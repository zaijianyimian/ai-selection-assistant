use super::*;
use std::{collections::BTreeMap, sync::Mutex};

#[derive(Default)]
pub struct Fake {
    values: Mutex<BTreeMap<(String, String), String>>,
    writes: Mutex<Vec<Vec<String>>>,
    pub(crate) fail: Mutex<Vec<usize>>,
    add_concurrent: Mutex<bool>,
    fail_readback: Mutex<bool>,
    next_read_fails: Mutex<bool>,
}
impl Fake {
    fn put(&self, schema: &str, key: &str, value: &str) {
        self.values
            .lock()
            .unwrap()
            .insert((schema.into(), key.into()), value.into());
    }
    fn paths(&self, paths: &[&str]) {
        self.put(
            SCHEMA,
            "custom-keybindings",
            &paths.to_variant().print(true),
        );
    }
}
impl Runner for Fake {
    fn run(&self, args: &[&str]) -> AppResult<String> {
        match args[0] {
            "list-schemas" => Ok(format!("{SCHEMA}\norg.gnome.desktop.wm.keybindings")),
            "list-relocatable-schemas" => Ok(ENTRY.into()),
            "list-keys" => Ok(if args[1] == SCHEMA {
                "custom-keybindings"
            } else {
                "close"
            }
            .into()),
            "get" => {
                if std::mem::take(&mut *self.next_read_fails.lock().unwrap()) {
                    return Err(failure("Injected readback failure"));
                }
                Ok(self
                    .values
                    .lock()
                    .unwrap()
                    .get(&(args[1].into(), args[2].into()))
                    .cloned()
                    .unwrap_or_else(|| {
                        if args[2] == "custom-keybindings" || args[2] == "close" {
                            "@as []".into()
                        } else {
                            "''".into()
                        }
                    }))
            }
            "set" => {
                let mut writes = self.writes.lock().unwrap();
                writes.push(args.iter().map(|s| s.to_string()).collect());
                if self.fail.lock().unwrap().contains(&writes.len()) {
                    return Err(failure("Injected desktop failure"));
                }
                self.put(args[1], args[2], args[3]);
                if args[2] == "custom-keybindings"
                    && std::mem::take(&mut *self.fail_readback.lock().unwrap())
                {
                    *self.next_read_fails.lock().unwrap() = true;
                }
                if args[2] == "custom-keybindings" && *self.add_concurrent.lock().unwrap() {
                    *self.add_concurrent.lock().unwrap() = false;
                    let mut paths = array(args[3])?;
                    paths.push("/concurrent/".into());
                    self.put(
                        SCHEMA,
                        "custom-keybindings",
                        &paths.to_variant().print(true),
                    );
                }
                Ok(String::new())
            }
            _ => Err(failure("Unsupported fixture invocation")),
        }
    }
}

#[test]
fn session_routing_and_safe_mapping() {
    assert_eq!(
        select_backend(false, true, "ubuntu:GNOME", true),
        Backend::Gnome
    );
    assert_eq!(
        select_backend(false, true, "ubuntu:GNOME", false),
        Backend::Manual
    );
    assert_eq!(select_backend(false, true, "KDE", true), Backend::Manual);
    assert_eq!(
        select_backend(false, false, "GNOME", false),
        Backend::Native
    );
    assert_eq!(select_backend(true, false, "", false), Backend::Native);
    assert_eq!(accelerator("Ctrl+Shift+Q").unwrap(), "<Control><Shift>q");
    assert_eq!(
        accelerator("CommandOrControl+Alt+F12").unwrap(),
        "<Control><Alt>F12"
    );
    assert_eq!(
        normalized("<Shift><Primary>Q"),
        normalized("<Control><Shift>q")
    );
    assert!(accelerator("Ctrl+NumpadAdd").is_err());
    assert!(accelerator("Ctrl+Q;touch /tmp/injected").is_err());
    assert_eq!(
        string(&encode("'\\ 中文 $(no)\n")).unwrap(),
        "'\\ 中文 $(no)\n"
    );
    assert!(array("__import__('os')").is_err());
}
#[test]
fn inspection_is_read_only_and_enable_is_idempotent() {
    let runner = Fake::default();
    let desktop = Desktop(&runner);
    desktop.available().unwrap();
    desktop.snapshot().unwrap();
    assert!(runner.writes.lock().unwrap().is_empty());
    let change = desktop.prepare(Some("Ctrl+Shift+Q")).unwrap();
    desktop.apply(&change).unwrap();
    let change = desktop.prepare(Some("Ctrl+Shift+Q")).unwrap();
    desktop.apply(&change).unwrap();
    assert_eq!(desktop.paths().unwrap(), vec![PATH]);
    assert_eq!(desktop.snapshot().unwrap().command, COMMAND);
}
#[test]
fn ownership_external_edits_and_visible_conflicts() {
    let runner = Fake::default();
    let desktop = Desktop(&runner);
    runner.paths(&["/other/"]);
    runner.put(&Desktop::entry("/other/"), "binding", "'<Shift><Primary>Q'");
    assert_eq!(
        desktop.prepare(Some("Ctrl+Shift+Q")).unwrap_err().code,
        "shortcut_conflict"
    );
    runner.put(&Desktop::entry("/other/"), "binding", "''");
    runner.put(
        "org.gnome.desktop.wm.keybindings",
        "close",
        "['<Primary><Shift>q']",
    );
    assert!(desktop.prepare(Some("Ctrl+Shift+Q")).is_err());
    runner.put("org.gnome.desktop.wm.keybindings", "close", "[]");
    let enable = desktop.prepare(Some("Alt+Shift+T")).unwrap();
    desktop.apply(&enable).unwrap();
    runner.put(
        &Desktop::entry(PATH),
        "command",
        "'other-app --private-argument'",
    );
    let before = runner.writes.lock().unwrap().len();
    assert!(desktop.prepare(None).is_err());
    assert!(desktop.prepare(Some("Ctrl+Q")).is_err());
    assert!(desktop.rollback(&enable).is_err());
    assert_eq!(runner.writes.lock().unwrap().len(), before);
}
#[test]
fn partial_write_rollback_preserves_unrelated_and_concurrent_entries() {
    for point in 1..=4 {
        let runner = Fake::default();
        runner.paths(&["/other/"]);
        let desktop = Desktop(&runner);
        let change = desktop.prepare(Some("Ctrl+Shift+Q")).unwrap();
        *runner.fail.lock().unwrap() = vec![point];
        assert!(desktop.apply(&change).is_err());
        runner.fail.lock().unwrap().clear();
        let mut paths = desktop.paths().unwrap();
        paths.push("/new/".into());
        runner.put(
            SCHEMA,
            "custom-keybindings",
            &paths.to_variant().print(true),
        );
        desktop.rollback(&change).unwrap();
        assert_eq!(desktop.snapshot().unwrap(), change.before);
        assert_eq!(desktop.paths().unwrap(), vec!["/other/", "/new/"]);
    }
    let runner = Fake::default();
    *runner.add_concurrent.lock().unwrap() = true;
    let desktop = Desktop(&runner);
    desktop
        .apply(&desktop.prepare(Some("Ctrl+Q")).unwrap())
        .unwrap();
    assert!(desktop.paths().unwrap().contains(&"/concurrent/".into()));
}
#[test]
fn change_and_disable_never_retire_the_new_binding() {
    let runner = Fake::default();
    runner.paths(&["/other/"]);
    let desktop = Desktop(&runner);
    desktop
        .apply(&desktop.prepare(Some("Ctrl+Q")).unwrap())
        .unwrap();
    let change = desktop.prepare(Some("Ctrl+Alt+T")).unwrap();
    desktop.apply(&change).unwrap();
    assert_eq!(desktop.snapshot().unwrap().binding, "<Control><Alt>t");
    desktop.rollback(&change).unwrap();
    assert_eq!(desktop.snapshot().unwrap().binding, "<Control>q");
    desktop.apply(&desktop.prepare(None).unwrap()).unwrap();
    assert_eq!(desktop.paths().unwrap(), vec!["/other/"]);
    assert_eq!(desktop.snapshot().unwrap(), Snapshot::default());
}
#[test]
fn standalone_journal_recovers_after_failure_and_restart() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let runner = Fake::default();
    assert!(!load_metadata(&path).unwrap().enabled);
    *runner.fail.lock().unwrap() = vec![2];
    assert!(manage_with(&path, "Ctrl+Q", "enable", &runner).is_err());
    assert!(!recovery_path(&path).exists());
    assert!(!load_metadata(&path).unwrap().enabled);
    // Force a partial write and compensation failure; the journal must gate model requests.
    runner.writes.lock().unwrap().clear();
    *runner.fail.lock().unwrap() = vec![2, 3];
    assert!(manage_with(&path, "Ctrl+Q", "enable", &runner).is_err());
    assert!(recovery_path(&path).exists());
    assert!(crate::settings_commit::ensure_ready(&path, &AtomicBool::new(false)).is_err());
    runner.fail.lock().unwrap().clear();
    manage_with(&path, "Ctrl+Q", "recover", &runner).unwrap();
    assert!(!recovery_path(&path).exists());
    assert!(!load_metadata(&path).unwrap().enabled);
    manage_with(&path, "Ctrl+Q", "enable", &runner).unwrap();
    assert!(load_metadata(&path).unwrap().enabled);
    manage_with(&path, "Ctrl+Q", "disable", &runner).unwrap();
    assert!(!load_metadata(&path).unwrap().enabled);
    *runner.fail_readback.lock().unwrap() = true;
    assert!(manage_with(&path, "Ctrl+Q", "enable", &runner).is_err());
    assert!(!load_metadata(&path).unwrap().enabled);
    assert!(!recovery_path(&path).exists());
}

#[test]
fn missing_schemas_and_malformed_results_are_rejected_without_writes() {
    struct Bad;
    impl Runner for Bad {
        fn run(&self, _: &[&str]) -> AppResult<String> {
            Ok("invalid".into())
        }
    }
    assert!(Desktop(&Bad).available().is_err());
    assert!(Desktop(&Bad).snapshot().is_err());
}

#[test]
#[ignore = "Requires a disposable dbus-run-session and XDG_CONFIG_HOME; never use the user session"]
fn isolated_real_gsettings_persistence_and_external_changes() {
    assert_eq!(
        std::env::var("AI_SELECTION_TEST_GNOME_ISOLATED").as_deref(),
        Ok("1")
    );
    assert!(std::env::var("XDG_CONFIG_HOME")
        .unwrap()
        .contains("asa-shortcuts-integration-"));
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    let desktop = Desktop(&System);
    desktop.available().unwrap();
    let before = desktop.paths().unwrap();
    assert!(before.is_empty());
    manage(&path, "Ctrl+Shift+Q", "enable").unwrap();
    assert_eq!(Desktop(&System).snapshot().unwrap().command, COMMAND);
    let initial_paths = desktop.paths().unwrap();
    manage(&path, "Ctrl+Shift+Q", "enable").unwrap();
    assert_eq!(desktop.paths().unwrap(), initial_paths);
    // New adapter instance reads the persisted configuration, without registration or writes.
    assert!(Desktop(&System).snapshot().unwrap().listed);
    System
        .run(&["set", &Desktop::entry(PATH), "binding", "'<Alt>a'"])
        .unwrap();
    assert_eq!(Desktop(&System).snapshot().unwrap().binding, "<Alt>a");
    desktop.membership(false).unwrap();
    assert!(!Desktop(&System).snapshot().unwrap().listed);
    manage(&path, "Ctrl+Shift+Q", "reapply").unwrap();
    manage(&path, "Ctrl+Shift+Q", "disable").unwrap();
    assert_eq!(desktop.paths().unwrap(), before);
}

#[test]
fn old_settings_and_sync_payload_do_not_enable_or_export_desktop_management() {
    let config: crate::settings::Settings =
        serde_json::from_str(r#"{"shortcut":"Ctrl+Shift+Q"}"#).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    assert!(!load_metadata(&path).unwrap().enabled);
    let prefs = crate::sync::storage::prefs(&config);
    assert_eq!(prefs.len(), 3);
    assert!(!prefs.contains_key("shortcut"));
    assert!(!serde_json::to_string(&prefs).unwrap().contains("enabled"));
}
#[test]
fn process_errors_and_deadline_are_bounded() {
    assert!(run_program("/does-not-exist", &[], Duration::from_secs(1)).is_err());
    assert!(run_program("/usr/bin/false", &[], Duration::from_secs(1)).is_err());
    let start = Instant::now();
    assert!(run_program("/usr/bin/sleep", &["10"], Duration::from_millis(30)).is_err());
    assert!(start.elapsed() < Duration::from_secs(2));
    assert!(run_program(
        "/usr/bin/head",
        &["-c", "150000", "/dev/zero"],
        Duration::from_millis(100)
    )
    .is_err());
}
