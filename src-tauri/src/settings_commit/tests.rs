use super::*;
use crate::memory::{Observation, Profile, Topic};
use std::sync::Mutex;

#[derive(Default)]
struct Fake {
    faults: Mutex<Vec<&'static str>>,
    key: Mutex<Option<String>>,
    registered: Mutex<Vec<String>>,
    calls: Mutex<Vec<String>>,
}
impl Fake {
    fn with_faults(faults: Vec<&'static str>) -> Arc<Self> {
        Arc::new(Self {
            faults: Mutex::new(faults),
            key: Mutex::new(Some("fixture-old".into())),
            registered: Mutex::new(vec!["Ctrl+Shift+Q".into()]),
            ..Self::default()
        })
    }
}
impl Services for Fake {
    fn checkpoint(&self, name: &str) -> AppResult<()> {
        self.calls.lock().unwrap().push(name.into());
        let mut faults = self.faults.lock().unwrap();
        if faults.first().copied() == Some(name) {
            faults.remove(0);
            return Err(AppError::new("fixture", "Injected failure"));
        }
        Ok(())
    }
    fn read_key(&self) -> AppResult<Option<Zeroizing<String>>> {
        self.checkpoint("read_key")?;
        Ok(self.key.lock().unwrap().clone().map(Zeroizing::new))
    }
    fn write_key(&self, key: Option<&str>) -> AppResult<()> {
        // Intentionally mutate before failure to exercise compensation of partial vault writes.
        *self.key.lock().unwrap() = key.map(str::to_string);
        self.checkpoint("write_key")
    }
    fn register(&self, value: &str) -> AppResult<()> {
        self.checkpoint("register")?;
        self.registered.lock().unwrap().push(value.into());
        Ok(())
    }
    fn unregister(&self, value: &str) -> AppResult<()> {
        self.checkpoint("unregister")?;
        self.registered.lock().unwrap().retain(|s| s != value);
        Ok(())
    }
}
async fn populated(path: &Path) -> Memory {
    let db = Memory::open(path.into()).unwrap();
    let epoch = db.epoch().await.unwrap();
    let id = db
        .record(
            epoch,
            "fixture".into(),
            "manual".into(),
            vec![Observation {
                original: "Agent".into(),
                translation: "智能体".into(),
                domain: "Agent".into(),
                sense_summary: "自主系统".into(),
                language: "English".into(),
            }],
        )
        .await
        .unwrap()[0];
    db.finish(
        db.snapshot(false).await.unwrap(),
        Profile {
            topics: vec![Topic {
                domain: "Agent".into(),
                evidence_ids: vec![id],
            }],
        },
    )
    .await
    .unwrap();
    db
}
async fn state(db: &Memory) -> (i64, String) {
    (
        db.epoch().await.unwrap(),
        serde_json::to_string(&db.view("".into(), 0).await.unwrap()).unwrap(),
    )
}
fn old_settings() -> Settings {
    Settings {
        remember_terms: true,
        ..Settings::default()
    }
}
fn next_settings() -> Settings {
    Settings {
        remember_terms: false,
        shortcut: "Alt+Shift+T".into(),
        ..Settings::default()
    }
}

#[tokio::test]
async fn legacy_order_reproduces_profile_loss_before_failed_credential_operation() {
    let dir = tempfile::tempdir().unwrap();
    let db = populated(&dir.path().join("memory.sqlite3")).await;
    let before = state(&db).await;
    // This is the exact destructive sequence in the 0.1.1 save handler.
    db.invalidate().await.unwrap();
    let fake = Fake::with_faults(vec!["read_key"]);
    assert!(fake.read_key().is_err());
    assert_ne!(state(&db).await, before);
    assert!(db.view("".into(), 0).await.unwrap().versions.is_empty());
}

#[tokio::test]
async fn every_precommit_failure_preserves_configuration_profiles_and_external_resources() {
    for fault in [
        "stage",
        "read_key",
        "register",
        "write_key",
        "publish",
        "memory_invalidate",
        "memory_commit",
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        settings::persist(&path, &old_settings()).unwrap();
        let bytes = fs::read(&path).unwrap();
        let db = populated(&dir.path().join("memory.sqlite3")).await;
        let before = state(&db).await;
        let fake = Fake::with_faults(vec![fault]);
        let recovering = AtomicBool::new(false);
        let result = commit(
            &path,
            &old_settings(),
            &next_settings(),
            Some(Zeroizing::new("fixture-new".into())),
            false,
            Some(db.clone()),
            fake.clone(),
            &recovering,
        )
        .await;
        assert!(result.is_err(), "{fault}");
        assert_eq!(state(&db).await, before, "{fault}");
        assert_eq!(fs::read(&path).unwrap(), bytes, "{fault}");
        assert_eq!(
            *fake.key.lock().unwrap(),
            Some("fixture-old".into()),
            "{fault}"
        );
        assert_eq!(
            *fake.registered.lock().unwrap(),
            vec!["Ctrl+Shift+Q"],
            "{fault}"
        );
        assert!(ensure_ready(&path, &recovering).is_ok(), "{fault}");
        assert!(!fs::read_dir(dir.path()).unwrap().any(|p| p
            .unwrap()
            .path()
            .extension()
            .is_some_and(|x| x == "tmp")));
    }
}

#[tokio::test]
async fn compensation_failure_survives_restart_and_requires_explicit_credential_recovery() {
    for faults in [
        vec!["memory_commit", "restore_file"],
        vec!["publish", "write_key"],
        vec!["publish", "unregister"],
    ] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        settings::persist(&path, &old_settings()).unwrap();
        let db = populated(&dir.path().join("memory.sqlite3")).await;
        let before = state(&db).await;
        let recovering = AtomicBool::new(false);
        let fake = Fake::with_faults(faults);
        let err = commit(
            &path,
            &old_settings(),
            &next_settings(),
            Some(Zeroizing::new("fixture-new".into())),
            false,
            Some(db.clone()),
            fake.clone(),
            &recovering,
        )
        .await
        .err()
        .unwrap();
        assert_eq!(err.code, "settings_recovery");
        assert_eq!(state(&db).await, before);
        assert!(ensure_ready(&path, &recovering).is_err());
        let restarted = AtomicBool::new(false);
        assert!(ensure_ready(&path, &restarted).is_err());
        assert!(!fs::read_to_string(marker(&path))
            .unwrap()
            .contains("fixture"));
        let good = fake.clone();
        assert!(commit(
            &path,
            &old_settings(),
            &old_settings(),
            None,
            false,
            Some(db.clone()),
            good.clone(),
            &restarted
        )
        .await
        .is_err());
        commit(
            &path,
            &old_settings(),
            &old_settings(),
            Some(Zeroizing::new("fixture-recovered".into())),
            false,
            Some(db.clone()),
            good,
            &restarted,
        )
        .await
        .unwrap();
        assert!(ensure_ready(&path, &restarted).is_ok());
        assert_eq!(*fake.registered.lock().unwrap(), vec!["Ctrl+Shift+Q"]);
        assert_eq!(state(&db).await, before);
    }
}

#[tokio::test]
async fn first_save_and_absent_database_do_not_touch_unmodified_resources() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let fake = Fake::with_faults(vec![]);
    let recovering = AtomicBool::new(false);
    let current = Settings::default();
    let config = Settings {
        model: "local-model".into(),
        ..current.clone()
    };
    commit(
        &path,
        &current,
        &config,
        None,
        false,
        None,
        fake.clone(),
        &recovering,
    )
    .await
    .unwrap();
    assert_eq!(settings::load(&path).unwrap(), config);
    assert!(!fake
        .calls
        .lock()
        .unwrap()
        .iter()
        .any(|s| ["register", "unregister", "write_key"].contains(&s.as_str())));
    assert!(!fs::read_to_string(&path).unwrap().contains("fixture-old"));
    let toggled = Settings {
        remember_terms: true,
        ..config.clone()
    };
    assert!(commit(
        &path,
        &config,
        &toggled,
        None,
        false,
        None,
        fake.clone(),
        &recovering
    )
    .await
    .is_err());
    assert_eq!(settings::load(&path).unwrap(), config);
}

#[tokio::test]
async fn first_save_failure_removes_published_file_and_success_invalidates_old_writes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let db = populated(&dir.path().join("memory.sqlite3")).await;
    let before = state(&db).await;
    let recovering = AtomicBool::new(false);
    let result = commit(
        &path,
        &old_settings(),
        &next_settings(),
        None,
        false,
        Some(db.clone()),
        Fake::with_faults(vec!["memory_commit"]),
        &recovering,
    )
    .await;
    assert!(result.is_err());
    assert!(!path.exists());
    assert_eq!(state(&db).await, before);
    let old_snapshot = db.snapshot(false).await; // Already analyzed; create new pending data below.
    assert!(old_snapshot.is_err());
    let id = db.view("".into(), 0).await.unwrap().terms[0].id;
    db.feedback(id, "known".into()).await.unwrap();
    let snapshot = db.snapshot(false).await.unwrap();
    let profile = Profile {
        topics: vec![Topic {
            domain: "Agent".into(),
            evidence_ids: vec![id],
        }],
    };
    let out = commit(
        &path,
        &old_settings(),
        &next_settings(),
        None,
        false,
        Some(db.clone()),
        Fake::with_faults(vec![]),
        &recovering,
    )
    .await
    .unwrap();
    assert!(out.invalidated);
    assert!(db
        .record(before.0, "late".into(), "manual".into(), vec![])
        .await
        .is_err());
    assert!(db.finish(snapshot, profile).await.is_err());
    let reopened = Memory::open(dir.path().join("memory.sqlite3")).unwrap();
    let view = reopened.view("".into(), 0).await.unwrap();
    assert_eq!(view.total, 1);
    assert!(view.versions.is_empty());
    assert_eq!(settings::load(&path).unwrap(), next_settings());
}

#[tokio::test]
async fn cleanup_and_key_presence_failures_are_postcommit_warnings() {
    for fault in ["unregister", "read_key"] {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let current = Settings::default();
        let next = Settings {
            shortcut: "Alt+Shift+T".into(),
            ..current.clone()
        };
        let result = commit(
            &path,
            &current,
            &next,
            None,
            false,
            None,
            Fake::with_faults(vec![fault]),
            &AtomicBool::new(false),
        )
        .await
        .unwrap();
        assert!(!result.warnings.is_empty());
        assert_eq!(settings::load(&path).unwrap(), next);
    }
}

#[tokio::test]
async fn serialized_saves_publish_latest_configuration() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let current = tokio::sync::Mutex::new(Settings::default());
    let fake = Fake::with_faults(vec![]);
    let recovering = AtomicBool::new(false);
    let save = |name: &'static str| {
        let path = &path;
        let current = &current;
        let fake = fake.clone();
        let recovering = &recovering;
        async move {
            let mut current = current.lock().await;
            let next = Settings {
                model: name.into(),
                ..current.clone()
            };
            commit(path, &current, &next, None, false, None, fake, recovering)
                .await
                .unwrap();
            *current = next;
        }
    };
    tokio::join!(save("one"), save("two"));
    assert_eq!(settings::load(&path).unwrap(), *current.lock().await);
}

#[tokio::test]
async fn restart_recovery_compares_against_original_switches_after_file_restore_failure() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    settings::persist(&path, &old_settings()).unwrap();
    let db = populated(&dir.path().join("memory.sqlite3")).await;
    let before = db.epoch().await.unwrap();
    let fake = Fake::with_faults(vec!["memory_commit", "restore_file"]);
    assert!(commit(
        &path,
        &old_settings(),
        &next_settings(),
        None,
        false,
        Some(db.clone()),
        fake.clone(),
        &AtomicBool::new(false)
    )
    .await
    .is_err());
    let loaded = settings::load(&path).unwrap();
    assert!(!loaded.remember_terms);
    assert_eq!(db.view("".into(), 0).await.unwrap().versions.len(), 1);
    let restarted = AtomicBool::new(false);
    let out = commit(
        &path,
        &loaded,
        &loaded,
        Some(Zeroizing::new("fixture-recovered".into())),
        false,
        Some(db.clone()),
        fake,
        &restarted,
    )
    .await
    .unwrap();
    assert!(out.invalidated);
    assert!(db.epoch().await.unwrap() > before);
    let view = db.view("".into(), 0).await.unwrap();
    assert!(view.versions.is_empty());
    assert_eq!(view.total, 1);
    assert!(ensure_ready(&path, &restarted).is_ok());
}
