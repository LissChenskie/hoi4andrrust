//! Local data preparation; does not need a GPU or an Android device.
use hoi4_mobile::research_import::{Installation, ResearchDataset, atomic_json};
use std::{
    fs,
    path::{Path, PathBuf},
};
fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    if !from.is_dir() {
        return Ok(());
    }
    fs::create_dir_all(to).map_err(|e| e.to_string())?;
    let mut entries = fs::read_dir(from)
        .map_err(|e| e.to_string())?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| e.to_string())?;
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let src = e.path();
        let dest = to.join(e.file_name());
        if e.file_type().map_err(|e| e.to_string())?.is_symlink() {
            return Err(format!(
                "Symbolic links are not exported: {}",
                src.display()
            ));
        }
        if src.is_dir() {
            copy_tree(&src, &dest)?;
        } else if src.extension().and_then(|s| s.to_str()).is_some_and(|s| {
            matches!(
                s,
                "txt" | "lua" | "yml" | "gui" | "gfx" | "csv" | "mod" | "dlc" | "json"
            )
        }) {
            fs::copy(&src, &dest).map_err(|e| format!("{}: {e}", src.display()))?;
        }
    }
    Ok(())
}
fn arg(args: &[String], key: &str) -> Option<String> {
    args.windows(2).find(|w| w[0] == key).map(|w| w[1].clone())
}
fn repeated(args: &[String], key: &str) -> Vec<String> {
    args.windows(2)
        .filter(|w| w[0] == key)
        .map(|w| w[1].clone())
        .collect()
}
fn export(args: &[String]) -> Result<(), String> {
    if args.iter().any(|s| s == "--no-dlc") && args.iter().any(|s| s == "--dlc") {
        return Err("Choose --no-dlc or --dlc".into());
    }
    let game = PathBuf::from(arg(args, "--game").ok_or("Specify --game <HOI4 installation>")?)
        .canonicalize()
        .map_err(|e| e.to_string())?;
    if !game.join("common/technologies").is_dir() {
        return Err("Game folder has no common/technologies".into());
    }
    let output = PathBuf::from(arg(args, "--out").unwrap_or("data/hoi4".into()));
    let parent = output
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let parent = parent.canonicalize().map_err(|e| e.to_string())?;
    let output = parent.join(output.file_name().ok_or("Invalid output folder")?);
    if output.join("Cargo.toml").is_file() || output.join("hoi4.exe").is_file() {
        return Err("The output must be a data folder, not a project/game root".into());
    }
    if game.starts_with(&output) || output.starts_with(&game) {
        return Err("Export folder must be outside the HOI4 installation".into());
    }
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_err(|e| e.to_string())?
        .as_millis();
    let staging = parent.join(format!("hoi4-export-{nonce}.tmp"));
    fs::create_dir(&staging).map_err(|e| e.to_string())?;
    let operation = (|| {
        for rel in ["common", "history", "events", "localisation", "interface"] {
            copy_tree(&game.join(rel), &staging.join(rel))?;
        }
        fs::create_dir_all(staging.join("map")).map_err(|e| e.to_string())?;
        for name in [
            "provinces.bmp",
            "rivers.bmp",
            "definition.csv",
            "adjacencies.csv",
            "adjacency_rules.txt",
            "default.map",
            "positions.txt",
        ] {
            let src = game.join("map").join(name);
            if src.is_file() {
                fs::copy(src, staging.join("map").join(name)).map_err(|e| e.to_string())?;
            }
        }
        if game.join("gfx/flags").is_dir() {
            fs::create_dir_all(staging.join("gfx/flags")).map_err(|e| e.to_string())?;
            for e in fs::read_dir(game.join("gfx/flags")).map_err(|e| e.to_string())? {
                let p = e.map_err(|e| e.to_string())?.path();
                if p.extension().is_some_and(|s| s == "tga") {
                    fs::copy(&p, staging.join("gfx/flags").join(p.file_name().unwrap()))
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        let mut layers = Vec::new();
        for (n, path) in repeated(args, "--layer").iter().enumerate() {
            let src = Path::new(path);
            if !src.is_dir() {
                return Err(format!("Layer must be an unpacked directory: {path}"));
            }
            let rel = format!("research_layers/{n:03}");
            copy_tree(src, &staging.join(&rel))?;
            layers.push(rel);
        }
        fn dlcs(dir: &Path, names: &mut Vec<String>) -> Result<(), String> {
            if !dir.is_dir() {
                return Ok(());
            }
            for e in fs::read_dir(dir).map_err(|e| e.to_string())? {
                let e = e.map_err(|e| e.to_string())?;
                let p = e.path();
                if e.file_type().map_err(|e| e.to_string())?.is_symlink() {
                    continue;
                }
                if p.is_dir() {
                    dlcs(&p, names)?;
                } else if p.extension().is_some_and(|s| s == "dlc") {
                    match fs::read_to_string(&p)
                        .map_err(|e| e.to_string())
                        .and_then(|s| hoi4_mobile::pdx::parse_pdx(&s))
                    {
                        Ok(b) => {
                            if let Some(name) = hoi4_mobile::pdx::atom(&b, "name") {
                                names.push(name.into());
                            }
                        }
                        Err(e) => {
                            eprintln!("DLC metadata {}: {e}", p.display());
                            names.push(format!("UNPARSED: {}", p.display()));
                        }
                    }
                }
            }
            Ok(())
        }
        let mut installed = Vec::new();
        dlcs(&game.join("dlc"), &mut installed)?;
        installed.sort();
        installed.dedup();
        let enabled = repeated(args, "--dlc");
        let mut version = arg(args, "--version");
        if version.is_none() {
            if let Ok(s) = fs::read_to_string(game.join("launcher-settings.json")) {
                if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                    version = v
                        .get("rawVersion")
                        .or_else(|| v.get("version"))
                        .and_then(|v| v.as_str())
                        .map(str::to_owned);
                }
            }
        }
        let install = Installation {
            version,
            checksum: arg(args, "--checksum"),
            executable_sha256: arg(args, "--exe-sha256"),
            enabled_dlcs: enabled.clone(),
            installed_dlcs: installed,
            dlc_selection_verified: args.iter().any(|s| s == "--no-dlc" || s == "--dlc"),
            load_order_verified: layers.is_empty() || args.iter().any(|s| s == "--layers-verified"),
            data_layers: layers.clone(),
            scenario: arg(args, "--scenario"),
            exported_at: Some(nonce.to_string()),
        };
        atomic_json(&staging.join("research-import.json"), &install)?;
        // Existing military import consumes the same explicit order and DLC selection.
        let quote = |s: &str| format!("\"{}\"", s.replace('\\', "/").replace('"', "\\\""));
        fs::write(
            staging.join("military_profile.txt"),
            format!(
                "dlcs = {{ {} }}\ndata_layers = {{ {} }}\n",
                enabled
                    .iter()
                    .map(|s| quote(s))
                    .collect::<Vec<_>>()
                    .join(" "),
                layers
                    .iter()
                    .map(|s| quote(s))
                    .collect::<Vec<_>>()
                    .join(" ")
            ),
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(e) = operation {
        let _ = fs::remove_dir_all(&staging);
        return Err(e);
    }
    let backup = parent.join(format!("hoi4-backup-{nonce}"));
    if output.exists() {
        fs::rename(&output, &backup).map_err(|e| e.to_string())?;
    }
    if let Err(e) = fs::rename(&staging, &output) {
        if backup.exists() {
            let _ = fs::rename(&backup, &output);
        }
        return Err(e.to_string());
    }
    println!("Local source export: {}", output.display());
    if backup.exists() {
        println!("Previous data preserved: {}", backup.display());
    }
    compile(&output)
}
fn compile(root: &Path) -> Result<(), String> {
    let data = ResearchDataset::load(root, false)?;
    let report = data.audit(root);
    atomic_json(&root.join("research-report.json"), &report)?;
    println!(
        "Imported technology definitions: {}",
        data.registry.technologies.len()
    );
    println!("Country histories: {}", data.histories.len());
    println!("Diagnostics: {}", data.diagnostics.len());
    println!("Report: {}", root.join("research-report.json").display());
    println!("No original-engine timing comparisons have been performed.");
    Ok(())
}
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let result=match args.first().map(String::as_str){Some("export")=>export(&args[1..]),Some("import"|"audit")=>compile(Path::new(args.get(1).map(String::as_str).unwrap_or("data/hoi4"))),_=>Err("Usage: hoi4-research export --game <folder> [--out data/hoi4] [--dlc <exact name> | --no-dlc] [--layer <directory>] | import [data/hoi4] | audit [data/hoi4]".into())};
    if let Err(e) = result {
        eprintln!("Research import failed: {e}");
        std::process::exit(1);
    }
}
