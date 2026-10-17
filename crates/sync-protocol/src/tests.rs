use super::*;
fn term() -> Term {
    Term {
        original: " Agent ".into(),
        translation: "智能体".into(),
        domain: "Agent".into(),
        sense: "自主执行任务的系统".into(),
        language: "English".into(),
    }
}
#[test]
fn identity_matches_existing_normalization_and_separates_meanings() {
    let a = term();
    let mut b = a.clone();
    b.original = "agent".into();
    b.translation = "其他译名".into();
    assert_eq!(a.key(), b.key());
    b.sense = "另一词义".into();
    assert_ne!(a.key(), b.key());
    assert_eq!(
        legacy_event_id("job", &a.key()),
        legacy_event_id("job", &a.key())
    );
    assert_ne!(
        legacy_event_id("job", &a.key()),
        legacy_event_id("other", &a.key())
    );
}
#[test]
fn strict_wire_rejects_secrets_and_invalid_preferences() {
    let mut t = serde_json::to_value(term()).unwrap();
    t["apiKey"] = "secret".into();
    assert!(serde_json::from_value::<Term>(t).is_err());
    assert!(serde_json::from_str::<Action>(
        r#"{"type":"preference","field":"baseUrl","baseRevision":0,"value":"https://bad.test"}"#
    )
    .is_err());
    assert!(!Action::Preference {
        field: Preference::Theme,
        base_revision: 0,
        value: "evil".into()
    }
    .validate());
    assert!(serde_json::from_str::<Login>(
        r#"{"protocol":1,"password":"fixture","deviceName":"device","sourceText":"hidden"}"#
    )
    .is_err());
}
#[test]
fn unicode_lengths_and_identity_are_bounded() {
    let mut t = term();
    t.original = "中".repeat(120);
    assert!(t.validate());
    t.original.push('中');
    assert!(!t.validate());
    t = term();
    t.sense = "中".repeat(120);
    assert!(t.validate());
    t.sense.push('中');
    assert!(!t.validate());
    assert!(!valid_id("../../file"));
    assert!(!valid_id(""));
}
#[test]
fn golden_wire_round_trip() {
    let wire = include_str!("../tests/observe.json");
    let p: Push = serde_json::from_str(wire).unwrap();
    assert!(p.validate());
    assert_eq!(
        serde_json::to_value(&p).unwrap(),
        serde_json::from_str::<serde_json::Value>(wire).unwrap()
    );
    let mut oversized = p.clone();
    oversized.operations = vec![p.operations[0].clone(); 101];
    assert!(!oversized.validate());
    oversized.protocol = 2;
    assert!(!oversized.validate());
}

#[test]
fn frozen_identity_fixture() {
    assert_eq!(
        term().key(),
        "t1:4405a5a84182d18106c30785a05ac04f49424197c501230533970f3fa240250d"
    );
}

#[test]
fn protocol_response_fixtures_round_trip() {
    fn round<T: serde::de::DeserializeOwned + Serialize>(wire: &str) {
        let value: T = serde_json::from_str(wire).unwrap();
        assert_eq!(
            serde_json::to_value(value).unwrap(),
            serde_json::from_str::<serde_json::Value>(wire).unwrap()
        );
    }
    round::<Page>(include_str!("../tests/page.json"));
    round::<Failure>(include_str!("../tests/conflict.json"));
    round::<Reset>(include_str!("../tests/reset.json"));
    let p: Push = serde_json::from_str(include_str!("../tests/observe.json")).unwrap();
    let mut p = p;
    p.operations[0].generation = -1;
    assert!(!p.validate());
}
