//! Import a subset of Clausewitz HOI4 technology declarations from the
//! player's own installation (data/hoi4/common/technologies/*.txt).
//! Unknown conditions fail closed. Unknown structural data is reported rather
//! than silently claiming a bit-perfect interpretation of all PDXScript.
use crate::research::{Requirement, ResearchCatalogue, ResearchRules, Technology};
use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub enum Value { Atom(String), Block(Vec<Entry>) }
#[derive(Debug, Clone)]
pub struct Entry { pub key: String, pub value: Option<Value> }
impl Entry {
    fn atom(&self) -> Option<&str> {
        match self.value.as_ref()? { Value::Atom(s)=>Some(s), _=>None }
    }
    fn block(&self) -> Option<&[Entry]> {
        match self.value.as_ref()? { Value::Block(b)=>Some(b), _=>None }
    }
}
fn find<'a>(items: &'a [Entry], name: &str) -> Option<&'a Entry> {
    items.iter().find(|e| e.key==name)
}
fn atom<'a>(items: &'a [Entry], name: &str) -> Option<&'a str> {
    find(items,name)?.atom()
}
fn number(items: &[Entry], name: &str) -> Option<f64> {
    atom(items,name)?.parse().ok()
}
fn lex(source: &str) -> Result<Vec<String>,String> {
    let mut out=Vec::new();
    let mut chars=source.chars().peekable();
    while let Some(c)=chars.next() {
        if c.is_whitespace() {continue;}
        if c=='#' {
            for v in chars.by_ref(){if v=='\n'{break;}}
            continue;
        }
        if matches!(c,'{'|'}'|'=') {out.push(c.to_string());continue;}
        if c=='"' {
            let mut text=String::new();let mut closed=false;
            while let Some(next)=chars.next(){
                if next=='"'{closed=true;break;}
                if next=='\\' {
                    if let Some(escaped)=chars.next(){text.push(escaped);}
                } else {text.push(next);}
            }
            if !closed {return Err("Unterminated quoted string".into());}
            out.push(text);continue;
        }
        let mut t=c.to_string();
        while let Some(&next)=chars.peek() {
            if next.is_whitespace()||matches!(next,'{'|'}'|'='|'#'|'"'){break;}
            t.push(next);chars.next();
        }
        out.push(t);
    }
    Ok(out)
}
fn entries(tokens: &[String], pos: &mut usize, block: bool) -> Result<Vec<Entry>,String>{
    let mut out=Vec::new();
    while *pos<tokens.len(){
        if tokens[*pos]=="}"{
            if !block{return Err("Unexpected closing brace".into());}
            *pos+=1;return Ok(out);
        }
        if tokens[*pos]=="{" || tokens[*pos]=="=" {return Err("Expected key".into());}
        let key=tokens[*pos].clone();*pos+=1;
        let value=if tokens.get(*pos).is_some_and(|v|v=="="){
            *pos+=1;
            if tokens.get(*pos).is_some_and(|v|v=="{"){
                *pos+=1;Some(Value::Block(entries(tokens,pos,true)?))
            }else{
                let Some(value)=tokens.get(*pos) else {return Err("Expected value".into());};
                if value=="}"{return Err("Expected atom".into());}
                *pos+=1;Some(Value::Atom(value.clone()))
            }
        }else{
            // Brace blocks can be part of bare value lists.
            if tokens.get(*pos).is_some_and(|v|v=="{"){
                *pos+=1;Some(Value::Block(entries(tokens,pos,true)?))
            }else{None}
        };
        out.push(Entry{key,value});
    }
    if block {Err("Unclosed brace".into())}else{Ok(out)}
}
pub fn parse_pdx(source:&str)->Result<Vec<Entry>,String>{
    let tokens=lex(source)?;let mut pos=0;entries(&tokens,&mut pos,false)
}
fn names(items:&[Entry])->BTreeSet<String>{
    items.iter().filter_map(|e|{
        match &e.value {
            None=>Some(e.key.clone()),
            Some(Value::Atom(x)) if x=="1"||x=="yes"=>Some(e.key.clone()),
            _=>None,
        }
    }).collect()
}
fn condition(items:&[Entry])->Requirement {
    let mut conditions=Vec::new();
    for item in items {
        let v=match item.key.as_str(){
            "always"=>Requirement::Always(item.atom()==Some("yes")),
            "has_country_flag"=>item.atom().map_or(
                Requirement::Unsupported("has_country_flag missing value".into()),
                |v|Requirement::HasCountryFlag(v.into())),
            "has_tech"|"has_technology"=>item.atom().map_or(
                Requirement::Unsupported("has_tech missing value".into()),
                |v|Requirement::HasTechnology(v.into())),
            "AND"|"and"=>item.block().map_or(
                Requirement::Unsupported("AND block missing".into()),
                condition),
            "OR"|"or"=>item.block().map_or(
                Requirement::Unsupported("OR block missing".into()),
                |block|Requirement::Or(block.iter().map(|x|condition(std::slice::from_ref(x))).collect())),
            "NOT"|"not"=>item.block().map_or(
                Requirement::Unsupported("NOT block missing".into()),
                |block|Requirement::Not(Box::new(condition(block)))),
            other=>Requirement::Unsupported(other.into()),
        };
        conditions.push(v);
    }
    Requirement::And(conditions)
}
fn collect_unlocks(items: &[Entry])->BTreeSet<String>{
    let mut out=BTreeSet::new();
    for entry in items {
        if matches!(entry.key.as_str(),"enable_equipments"|"enable_subunits"
            |"enable_equipment_modules"|"enable_building"|"enable_tactic") {
            if let Some(sub)=entry.block() {
                for name in names(sub) {out.insert(format!("{}:{}",entry.key,name));}
            }else if let Some(name)=entry.atom() {
                out.insert(format!("{}:{}",entry.key,name));
            }
        }
    }
    out
}
#[derive(Default,Debug)]
pub struct ImportReport {
    pub parsed_files:usize,
    pub imported_technologies:usize,
    pub diagnostics:Vec<String>,
}
pub fn parse_technology_file(source:&str, catalogue:&mut ResearchCatalogue)
    ->Result<usize,String> {
    let root=parse_pdx(source)?;
    let mut parsed=0;
    let mut incoming:Vec<(String,String)>=Vec::new();
    for top in &root {
        if top.key!="technologies" {continue;}
        let Some(nodes)=top.block()else{continue;};
        for item in nodes {
            let Some(fields)=item.block()else{continue;};
            let mut tech=Technology::sample(&item.key,
                number(fields,"start_year").unwrap_or(1936.0).max(0.0) as u32,
                number(fields,"research_cost").unwrap_or(1.0));
            if let Some(block)=find(fields,"categories").and_then(Entry::block) {
                tech.categories=names(block);
            }
            if let Some(block)=find(fields,"dependencies").and_then(Entry::block) {
                tech.prerequisites=names(block);
            }
            if let Some(block)=find(fields,"xor").and_then(Entry::block) {
                tech.xor=names(block);
            }
            if let Some(block)=find(fields,"allow").and_then(Entry::block) {
                tech.allow=condition(block);
            }
            if let Some(block)=find(fields,"allow_branch").and_then(Entry::block) {
                tech.allow_branch=condition(block);
            }
            if let Some(v)=find(fields,"ai_will_do").and_then(Entry::block) {
                tech.ai_weight=number(v,"factor").unwrap_or(1.0);
            }
            if let Some(v)=find(fields,"modifier").and_then(Entry::block) {
                for entry in v {
                    if let Some(value)=entry.atom().and_then(|x|x.parse::<f64>().ok()) {
                        tech.modifiers.insert(entry.key.clone(),value);
                    }
                }
            }
            tech.unlocks=collect_unlocks(fields);
            if let Some(folder)=find(fields,"folder").and_then(Entry::block) {
                if let Some(name)=atom(folder,"name") {tech.folder=name.into();}
                if let Some(pos)=find(folder,"position").and_then(Entry::block) {
                    tech.x=number(pos,"x").unwrap_or(0.0) as f32;
                    tech.y=number(pos,"y").unwrap_or(0.0) as f32;
                }
            }
            for path in fields.iter().filter(|e|e.key=="path") {
                if let Some(child)=path.block().and_then(|b|atom(b,"leads_to_tech")) {
                    incoming.push((child.into(),item.key.clone()));
                }
            }
            catalogue.insert(tech)?;
            parsed+=1;
        }
    }
    // Path edges may lead into a different file; finish resolving after all
    // files are imported in load_catalogue.
    for (child,parent) in incoming {
        if let Some(tech)=catalogue.technologies.get_mut(&child) {
            tech.prerequisites.insert(parent);
        }
    }
    Ok(parsed)
}
pub fn load_catalogue(directory:&Path)->Result<(ResearchCatalogue,ImportReport),String>{
    let mut catalogue=ResearchCatalogue::default();
    let mut report=ImportReport::default();
    let dir=fs::read_dir(directory).map_err(|e|format!("Cannot read {}: {e}",directory.display()))?;
    let mut paths:Vec<_>=dir.flatten().map(|entry|entry.path())
        .filter(|p|p.extension().and_then(|x|x.to_str())==Some("txt")).collect();
    paths.sort();
    // Resolve path dependencies in a second pass because parents and children
    // can be declared in different files.
    let mut sources=Vec::new();
    for path in paths{
        let content=fs::read_to_string(&path)
            .map_err(|e|format!("{}: {e}",path.display()))?;
        // Strip UTF-8 BOM from locally supplied scripts if present.
        let content=content.trim_start_matches('\u{feff}').to_owned();
        let root=parse_pdx(&content).map_err(|e|format!("{}: {e}",path.display()))?;
        let mut file_catalogue=ResearchCatalogue::default();
        let n=parse_technology_file(&content,&mut file_catalogue)
            .map_err(|e|format!("{}: {e}",path.display()))?;
        for tech in file_catalogue.technologies.into_values() {
            catalogue.insert(tech)?;
        }
        // Reinspect all declared path edges with all nodes loaded.
        let mut edges=Vec::new();
        for top in root.iter().filter(|e|e.key=="technologies") {
            if let Some(nodes)=top.block() {
                for node in nodes {
                    if let Some(block)=node.block() {
                        for e in block.iter().filter(|e|e.key=="path") {
                            if let Some(child)=e.block().and_then(|b|atom(b,"leads_to_tech")) {
                                edges.push((child.to_owned(),node.key.clone()));
                            }
                        }
                    }
                }
            }
        }
        sources.extend(edges);
        report.parsed_files+=1;report.imported_technologies+=n;
    }
    for (child,parent) in sources {
        if let Some(tech)=catalogue.technologies.get_mut(&child) {
            tech.prerequisites.insert(parent);
        } else {
            report.diagnostics.push(format!("Unresolved path: {parent} -> {child}"));
        }
    }
    catalogue.validate()?;
    if report.imported_technologies==0 {
        return Err("No technologies in source directory".into());
    }
    Ok((catalogue,report))
}

/// Read research defines by name without introducing an additional Lua
/// interpreter. Unknown lines keep explicit version defaults.
pub fn update_rules_from_defines(source:&str, rules:&mut ResearchRules){
    for line in source.lines(){
        let line=line.split("--").next().unwrap_or("");
        let Some((left,right))=line.split_once('=')else{continue;};
        let key=left.trim().rsplit('.').next().unwrap_or("").trim();
        let Some(raw)=right.split(',').next().and_then(|n|n.trim().parse::<f64>().ok())else{continue;};
        match key {
            "BASE_TECH_COST"|"RESEARCH_BASE_DAYS" if raw>0.0=>rules.base_tech_cost=raw,
            "BASE_YEAR_AHEAD_PENALTY_FACTOR" if raw>=0.0=>rules.year_ahead_penalty=raw,
            "BASE_RESEARCH_POINTS_SAVED" if raw>=0.0=>rules.max_saved_days=raw,
            _=>{},
        }
    }
}

pub fn load_country_research_start(source:&str)->(Option<usize>,BTreeSet<String>){
    let Ok(entries)=parse_pdx(source)else{return(None,BTreeSet::new());};
    let slots=atom(&entries,"set_research_slots").and_then(|v|v.parse().ok());
    let mut techs=BTreeSet::new();
    for e in entries.iter().filter(|e|e.key=="set_technology"){
        if let Some(block)=e.block(){
            for (key,raw) in block.iter().filter_map(|t|t.atom().map(|raw|(t.key.as_str(),raw))) {
                if raw=="1"||raw=="yes"{techs.insert(key.into());}
            }
        }
    }
    (slots,techs)
}

pub fn builtin_demo()->ResearchCatalogue {
    let mut result=ResearchCatalogue::default();
    // Standalone tutorial nodes only; never claim these are vanilla content.
    for (id,year,cost,folder,x,y,parent) in [
        ("basic_industry",1936,1.0,"industry",0.0,0.0,None),
        ("production_tools_1",1937,1.25,"industry",0.0,1.0,Some("basic_industry")),
        ("production_tools_2",1939,1.5,"industry",0.0,2.0,Some("production_tools_1")),
        ("basic_radio",1936,1.0,"electronics",0.0,0.0,None),
        ("signal_networks",1938,1.5,"electronics",0.0,1.0,Some("basic_radio")),
        ("basic_aviation",1936,1.25,"air",0.0,0.0,None),
        ("advanced_aviation",1939,1.5,"air",0.0,1.0,Some("basic_aviation")),
        ("basic_land_vehicles",1936,1.0,"land",0.0,0.0,None),
        ("motorisation",1938,1.5,"land",0.0,1.0,Some("basic_land_vehicles")),
    ] {
        let mut tech=Technology::sample(id,year,cost);
        tech.folder=folder.into();tech.x=x;tech.y=y;
        if let Some(parent)=parent{tech.prerequisites.insert(parent.into());}
        result.insert(tech).unwrap();
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn imports_paths_dependencies_xor_and_restrictions(){
        let raw=r#"
            technologies = {
                first = {
                    start_year = 1936
                    research_cost = 1.25
                    categories = { industry electronics }
                    path = { leads_to_tech = second research_cost_coeff = 1 }
                }
                second = {
                    research_cost = 1.5 start_year = 1939
                    dependencies = { first = 1 }
                    xor = { third = 1 }
                    allow = { has_country_flag = open }
                    folder = { name = industry position = { x = 2 y = 3 } }
                    ai_will_do = { factor = 1.5 }
                }
                third = { start_year = 1940 research_cost = 1 }
            }
        "#;
        let mut c=ResearchCatalogue::default();
        assert_eq!(parse_technology_file(raw,&mut c).unwrap(),3);
        c.validate().unwrap();
        let second=&c.technologies["second"];
        assert!(second.prerequisites.contains("first"));
        assert!(second.xor.contains("third"));
        assert_eq!(second.folder,"industry");
        assert_eq!((second.x,second.y),(2.0,3.0));
        assert_eq!(second.ai_weight,1.5);
        assert_eq!(c.technologies["first"].categories.len(),2);
    }
    #[test]
    fn focus_only_and_unsupported_triggers_fail_closed(){
        let mut c=ResearchCatalogue::default();
        parse_technology_file("technologies = { special = { allow = { always = no } research_cost = 1 } mystery = { allow = { unknown_condition = yes } research_cost = 1 } }",&mut c).unwrap();
        let player=crate::research::CountryResearch::new(1,&ResearchRules::default());
        assert!(!c.visible("special",&player));
        assert!(!c.visible("mystery",&player));
    }
    #[test]
    fn loads_defines_and_country_start_settings(){
        let mut rules=ResearchRules::default();
        update_rules_from_defines("NDefines.NTechnology.BASE_TECH_COST = 115, -- note\nNDefines.NTechnology.BASE_YEAR_AHEAD_PENALTY_FACTOR = 3,\nNDefines.NTechnology.BASE_RESEARCH_POINTS_SAVED = 24,",&mut rules);
        assert_eq!((rules.base_tech_cost,rules.year_ahead_penalty,rules.max_saved_days),(115.0,3.0,24.0));
        let (slots,tech)=load_country_research_start("set_research_slots = 3 set_technology = { radio = 1 other = 0 }");
        assert_eq!(slots,Some(3));assert!(tech.contains("radio"));assert!(!tech.contains("other"));
    }
    #[test]
    fn handles_unclosed_braces_and_comments(){
        assert!(parse_pdx("technologies = { broken = {").is_err());
        assert_eq!(parse_pdx("test = { #hello\n a = 1 }").unwrap().len(),1);
    }
}
