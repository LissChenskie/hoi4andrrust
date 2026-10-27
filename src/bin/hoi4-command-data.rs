//! Static commander inspection compiler. No GPU, assets, campaign saves or source writes.
use hoi4_mobile::command_data::{CommandData, Severity};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn files(root: &Path, directory: &str, extension: &str) -> Result<Vec<PathBuf>, String> {
    fn walk(p: &Path, extension: &str, out: &mut Vec<PathBuf>) -> Result<(), String> {
        if !p.exists() {
            return Ok(());
        }
        for e in fs::read_dir(p).map_err(|e| e.to_string())? {
            let e = e.map_err(|e| e.to_string())?;
            let kind = e.file_type().map_err(|e| e.to_string())?;
            if kind.is_symlink() {
                return Err(format!("Symlink not imported: {}", e.path().display()));
            }
            if kind.is_dir() {
                walk(&e.path(), extension, out)?;
            } else if e.path().extension().and_then(|s| s.to_str()) == Some(extension) {
                out.push(e.path());
            }
        }
        Ok(())
    }
    let mut out = Vec::new();
    walk(&root.join(directory), extension, &mut out)?;
    out.sort();
    Ok(out)
}
fn run() -> Result<bool, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() < 2 || args.len() > 3 {
        return Err("Usage: hoi4-command-data SOURCE_ROOT OUTPUT.json [SOURCE_VERSION] (one effective data directory; no automatic DLC selection)".into());
    }
    let root = Path::new(&args[0]);
    if !root.is_dir() {
        return Err("SOURCE_ROOT is not a directory".into());
    }
    let mut data = CommandData {
        source_version: args.get(2).cloned(),
        ..Default::default()
    };
    for (directory, extension) in [
        ("common/characters", "txt"),
        ("common/unit_leader", "txt"),
        ("localisation", "yml"),
    ] {
        for path in files(root, directory, extension)? {
            let file = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            match fs::read_to_string(&path) {
                Ok(text) if directory == "common/characters" => {
                    data.import_characters(&file, &text)
                }
                Ok(text) if directory == "common/unit_leader" => data.import_traits(&file, &text),
                Ok(text) => data.import_localization(&file, &text),
                Err(e) => data.diagnostic(
                    Severity::Error,
                    "read",
                    data.source(&file, "", 0),
                    e.to_string(),
                ),
            }
        }
    }
    if data.characters.is_empty() {
        data.diagnostic(
            Severity::Error,
            "empty_import",
            data.source("common/characters", "", 0),
            "No character definitions imported",
        );
    }
    data.validate();
    let out = Path::new(&args[1]);
    if out.exists()
        && out
            .canonicalize()
            .ok()
            .is_some_and(|p| p.starts_with(root.canonicalize().unwrap()))
    {
        return Err("Output must not overwrite an existing file within SOURCE_ROOT".into());
    }
    let encoded = serde_json::to_string_pretty(&data).map_err(|e| e.to_string())? + "\n";
    fs::write(out, encoded).map_err(|e| e.to_string())?;
    eprintln!(
        "{} source files, {} characters, {} profiles, {} traits, {} diagnostics",
        data.sources.len(),
        data.characters.len(),
        data.profiles.len(),
        data.traits.len(),
        data.diagnostics.len()
    );
    Ok(!data.has_errors())
}
fn main() {
    match run() {
        Ok(true) => {}
        Ok(false) => std::process::exit(2),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
