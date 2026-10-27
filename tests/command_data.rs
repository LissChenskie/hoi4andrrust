//! Authored corpus, not Paradox/mod data and not verified historical stats.
use hoi4_mobile::command_data::{CommandData, CommanderRole, Severity};
use std::{collections::BTreeSet, path::Path};
fn fixture() -> CommandData {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/command-data");
    let mut db = CommandData {
        source_version: Some("authored-test".into()),
        ..Default::default()
    };
    for name in ["basic", "multirole", "marshal"] {
        let p = format!("common/characters/{name}.txt");
        db.import_characters(&p, &std::fs::read_to_string(root.join(&p)).unwrap());
    }
    let p = "common/unit_leader/traits.txt";
    db.import_traits(p, &std::fs::read_to_string(root.join(p)).unwrap());
    for lang in ["english", "russian"] {
        let p = format!("localisation/test_l_{lang}.yml");
        db.import_localization(&p, &std::fs::read_to_string(root.join(&p)).unwrap());
    }
    db.validate();
    db
}
#[test]
fn deterministic_golden_commander_package() {
    let db = fixture();
    let encoded = serde_json::to_string_pretty(&db).unwrap() + "\n";
    assert_eq!(encoded, include_str!("fixtures/command-data/expected.json"));
    assert_eq!(db, serde_json::from_str(&encoded).unwrap());
    assert!(!db.has_errors());
}
#[test]
fn multirole_character_profiles_and_unknown_fields_survive() {
    let db = fixture();
    let c = &db.characters["TEST_multi"];
    assert_eq!(c.profile_ids.len(), 2);
    assert_eq!(
        db.profile(&c.id, CommanderRole::General)
            .unwrap()
            .stats
            .attack,
        Some(2)
    );
    assert_eq!(
        db.profile(&c.id, CommanderRole::FieldMarshal)
            .unwrap()
            .stats
            .attack,
        Some(4)
    );
    assert_eq!(c.unknown_fields[0].key, "country_leader");
    let c = &db.characters["TEST_general"];
    assert_eq!(c.source.line, 3);
    assert_eq!(c.name_key, "TEST_GENERAL_NAME");
    assert_eq!(c.unknown_fields[0].block().unwrap().len(), 2);
    assert!(
        db.sources["common/characters/basic.txt"]
            .text
            .starts_with('#')
    );
    assert_eq!(
        db.profile(&c.id, CommanderRole::General)
            .unwrap()
            .unknown_fields[0]
            .key,
        "future_role_field"
    );
}
#[test]
fn unknown_trait_and_localization_are_retained_with_diagnostics() {
    let mut db = fixture();
    assert!(
        db.profile("TEST_marshal", CommanderRole::FieldMarshal)
            .unwrap()
            .trait_ids
            .contains("unknown_trait")
    );
    assert!(
        db.diagnostics
            .iter()
            .any(|d| d.code == "unknown_trait" && d.source.file.ends_with("marshal.txt"))
    );
    assert_eq!(
        db.localization.get("russian", "TEST_GENERAL_NAME"),
        "Тестовый генерал"
    );
    assert_eq!(
        db.localization.get("russian", "TEST_MULTI_NAME"),
        "Test Multi"
    );
    assert_eq!(db.localization.get("russian", "unknown_key"), "unknown_key");
    assert_eq!(
        db.localization.get("russian", "test_organizer_desc"),
        "Own authored \"trait\" description"
    );
    let count = db.diagnostics.len();
    db.validate();
    assert_eq!(db.diagnostics.len(), count);
}
#[test]
fn invalid_numbers_duplicate_properties_and_ids_are_errors_not_zeroes() {
    let mut db = CommandData::default();
    db.import_characters("bad.txt", "characters={X={name=A name=B corps_commander={skill=-2 attack_skill=script_value defense_skill=2 planning_skill=3 logistics_skill=4}} X={name=C}}");
    assert!(db.has_errors());
    let p = db.profile("X", CommanderRole::General).unwrap();
    assert_eq!(p.stats.skill, None);
    assert_eq!(p.stats.attack, None);
    assert_eq!(db.characters["X"].name_key, "B");
    assert!(
        db.diagnostics
            .iter()
            .any(|d| d.code == "duplicate_property")
    );
    assert!(db.diagnostics.iter().any(|d| d.code == "duplicate_id"));
    assert!(db.sources["bad.txt"].entries[0].block().unwrap().len() == 2);
}
#[test]
fn malformed_files_and_localization_are_reported_without_disappearing() {
    let mut db = CommandData::default();
    db.import_characters("broken.txt", "characters={X={");
    assert_eq!(db.sources["broken.txt"].text, "characters={X={");
    db.import_localization(
        "broken.yml",
        "\u{feff}l_russian:\n a:0 \"unterminated\n b:0 \"value\" junk\n",
    );
    assert_eq!(
        db.diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .count(),
        3
    );
    assert!(!db.localization.contains("a"));
}
#[test]
fn selective_localization_does_not_retain_the_whole_database() {
    let mut db = CommandData::default();
    db.import_selected_localization(
        "generic.yml",
        "\u{feff}l_english:\n keep:0 \"Kept\"\n discard:0 \"Other\"",
        &BTreeSet::from(["keep".into()]),
    );
    assert_eq!(db.localization.get("russian", "keep"), "Kept");
    assert!(!db.localization.contains("discard"));
    assert!(db.sources.is_empty());
}
#[test]
fn canonical_reference_validation_rejects_missing_or_foreign_profiles() {
    let mut db = fixture();
    db.characters
        .get_mut("TEST_general")
        .unwrap()
        .profile_ids
        .push("missing".into());
    db.profiles.values_mut().next().unwrap().character_id = "missing".into();
    db.validate();
    assert!(
        db.diagnostics
            .iter()
            .any(|d| d.code == "broken_character_ref")
    );
    assert!(
        db.diagnostics
            .iter()
            .any(|d| d.code == "broken_profile_ref")
    );
}
