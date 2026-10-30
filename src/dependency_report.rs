//! Source-anchored focus/event/decision inventory. Not a parity claim.
use crate::{pdx::{self,Entry},research_import::ResearchDataset,research_script::ResearchScripts};
use serde_json::{Value,json};
use std::collections::BTreeSet;
fn traverse(
    b:&[Entry],cat:&ResearchScripts,
    refs:&mut BTreeSet<String>,missing:&mut BTreeSet<String>,
    dynamic:&mut BTreeSet<String>,seen:&mut BTreeSet<String>,depth:usize
) {
    if depth>32 || seen.len()>2048 {
        dynamic.insert("Script expansion exceeds safe limit".into());
        return;
    }
    for e in b {
        if e.atom().is_some_and(|v|v.contains(char::from(36))||v.contains('@')) {
            dynamic.insert(format!("{}:{} {}",e.span.line,e.span.column,e.key));
        }
        if e.key=="country_event" {
            let target=e.block().and_then(|b|pdx::atom(b,"id"));
            if let Some(target)=target {
                let key=format!("event:{target}:immediate");
                refs.insert(key.clone());
                if !cat.sources.contains_key(&key) {missing.insert(key);}
            }else {dynamic.insert("country_event unresolved ID".into());}
        }
        if let Some(effect)=cat.effects.get(&e.key) {
            let key=format!("effect:{}",e.key);
            refs.insert(key.clone());
            if seen.insert(key){
                traverse(effect,cat,refs,missing,dynamic,seen,depth+1);
            }
        }
        if let Some(body)=e.block() {
            traverse(body,cat,refs,missing,dynamic,seen,depth+1);
        }
    }
}
pub fn report(data:&ResearchDataset)->Value {
    let mut entries=Vec::new();
    for (key,src) in &data.registry.scripts.sources {
        if !matches!(src.kind.as_str(),"focus"|"decision"|"event"|"on_action"){continue;}
        let mut refs=BTreeSet::new();
        let mut missing=BTreeSet::new();
        let mut dynamic=BTreeSet::new();
        let mut seen=BTreeSet::new();
        traverse(&src.raw,&data.registry.scripts,&mut refs,&mut missing,
            &mut dynamic,&mut seen,0);
        let status=if !missing.is_empty()||!dynamic.is_empty() {
            "unresolved_dependencies"
        }else{
            "source_scanned_requires_runtime_preflight"
        };
        entries.push(json!({
            "key":key,"kind":src.kind,"source":src.source,
            "conditions":src.conditions,"effects":src.effects,
            "transitive_script_references":refs,
            "missing_references":missing,"dynamic_references":dynamic,
            "status":status,
        }));
    }
    json!({
        "installation":&data.installation,
        "fingerprint":&data.registry.fingerprint,
        "files":data.files.len(),
        "focus_trees":&data.registry.scripts.focus_trees,
        "sources":entries,
        "limitations":"Source inventory only. Condition/effect engine coverage, DLC gates, dynamic targets and actual consumers require gameplay verification; no full coverage claim."
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn a_missing_nested_event_is_not_marked_resolved() {
        let mut data=ResearchDataset {
            cache_version:7,registry:Default::default(),rules:Default::default(),
            histories:Default::default(),installation:Default::default(),files:vec![],
            diagnostics:vec![],from_cache:false,bookmarks:vec![],
            source_technology_ids:Default::default(),failed_source_files:Default::default(),
            rule_sources:Default::default(),define_values:Default::default(),
            source_country_ids:Default::default(),eof_terminated_sources:Default::default(),
            history_source_files:Default::default(),
        };
        data.registry.scripts.sources.insert("focus:a".into(),
            crate::research_script::ScriptSource {
                kind:"focus".into(),id:"a".into(),source:"authored/a:1".into(),
                conditions:vec![],
                effects:vec![],
                raw:pdx::parse_pdx("completion_reward={country_event={id=missing}}").unwrap(),
            });
        let report=report(&data);
        assert_eq!(report["sources"][0]["status"],"unresolved_dependencies");
        assert_eq!(report["sources"][0]["missing_references"][0],
            "event:missing:immediate");
    }
}
