//! Probes against a real exported data tree. Ignored by default; run with
//! `HOI4_DATA=/path/to/data/hoi4 cargo test --test real_data_probe -- --ignored --nocapture`.
use hoi4_mobile::research_import::ResearchDataset;
use std::path::Path;

#[test]
#[ignore]
fn events_referenced_by_focuses_are_found() {
    let root = std::env::var("HOI4_DATA").expect("set HOI4_DATA");
    let data = ResearchDataset::load(Path::new(&root), false).unwrap();
    let sources = &data.registry.scripts.sources;
    let events: Vec<_> = sources.iter().filter(|(_, s)| s.kind == "event").collect();
    println!("event sources: {}", events.len());
    for key in sources.keys().filter(|k| k.starts_with("event:generic.")).take(6) {
        println!("  key {key}");
    }
    let found = sources
        .values()
        .any(|s| s.kind == "event" && s.raw.iter().any(|e| e.key == "id" && e.atom() == Some("generic.3")));
    println!("generic.3 found by id lookup: {found}");
    assert!(found, "events without an immediate block must still be registered");
    for (k, v) in sources.iter().filter(|(k, _)| k.starts_with("event:generic.3:")) {
        let keys: Vec<_> = v.raw.iter().take(8).map(|e| e.key.as_str()).collect();
        println!("  {k}: raw entries={} first keys={keys:?} source={}", v.raw.len(), v.source);
    }
    let sample: Vec<_> = events.iter().filter(|(_, s)| !s.raw.iter().any(|e| e.key == "id")).take(3).map(|(k, s)| (k.as_str(), s.raw.len(), s.source.as_str())).collect();
    println!("sample without id: {sample:?}");
    let with_id: usize = events
        .iter()
        .filter(|(_, s)| s.raw.iter().any(|e| e.key == "id"))
        .count();
    println!("event sources whose raw has an id entry: {with_id}");
}
