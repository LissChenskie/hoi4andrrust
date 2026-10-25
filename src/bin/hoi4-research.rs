//! Local data preparation; does not need a GPU or an Android device.
use hoi4_mobile::research_import::{
    Installation, ResearchDataset, atomic_json, game_version, installation,
};
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
fn validate_profile(args: &[String]) -> Result<(), String> {
    if args.iter().any(|s| s == "--no-dlc") && args.iter().any(|s| s == "--dlc") {
        return Err("Choose --no-dlc or --dlc".into());
    }
    Ok(())
}
fn copy_metadata(game: &Path, output: &Path) -> Result<(), String> {
    for name in ["launcher-settings.json", "version.txt"] {
        if game.join(name).is_file() && game.join(name) != output.join(name) {
            fs::copy(game.join(name), output.join(name)).map_err(|e| format!("{name}: {e}"))?;
        }
    }
    Ok(())
}
fn refresh_metadata(root: &Path, args: &[String]) -> Result<(), String> {
    validate_profile(args)?;
    if !root.join("common/technologies").is_dir() {
        return Err("Export folder has no common/technologies".into());
    }
    if let Some(game) = arg(args, "--game") {
        copy_metadata(Path::new(&game), root)?;
    }
    let mut install = installation(root);
    if !root.join("research-import.json").is_file() {
        if let Some(profile) = fs::read_to_string(root.join("military_profile.txt"))
            .ok()
            .and_then(|s| hoi4_mobile::pdx::parse_pdx(&s).ok())
        {
            if let Some(dlcs) = hoi4_mobile::pdx::block(&profile, "dlcs") {
                install.enabled_dlcs = hoi4_mobile::pdx::names(dlcs).into_iter().collect();
                install.dlc_selection_verified = true;
            }
        }
    }
    // Earlier wrappers confused EXE ProductVersion with the patch number.
    if install.version_source.is_none() {
        if install.executable_version.is_none() {
            install.executable_version = install.version.take();
        }
        install.version = None;
    }
    if let Some(version) = arg(args, "--version") {
        install.version = Some(version);
        install.version_source = Some("command line --version".into());
    } else if let Some((version, source)) = game_version(root) {
        install.version = Some(version);
        install.version_source = Some(source);
    }
    if let Some(version) = arg(args, "--exe-version") {
        install.executable_version = Some(version);
    }
    if let Some(hash) = arg(args, "--exe-sha256") {
        install.executable_sha256 = Some(hash);
    }
    if let Some(checksum) = arg(args, "--checksum") {
        install.checksum = Some(checksum);
    }
    if let Some(scenario) = arg(args, "--scenario") {
        install.scenario = Some(scenario);
    }
    if args.iter().any(|s| s == "--no-dlc" || s == "--dlc") {
        install.enabled_dlcs = repeated(args, "--dlc");
        install.dlc_selection_verified = true;
        let path = root.join("military_profile.txt");
        let mut profile = fs::read_to_string(&path).unwrap_or_default();
        let names = install
            .enabled_dlcs
            .iter()
            .map(|s| format!("\"{}\"", s.replace('\\', "/").replace('"', "\\\"")))
            .collect::<Vec<_>>()
            .join(" ");
        profile.push_str(&format!("\ndlcs = {{ {names} }}\n"));
        fs::write(path, profile).map_err(|e| e.to_string())?;
    }
    if root.join("research-import.json").is_file() || args.iter().any(|s| s.starts_with("--")) {
        atomic_json(&root.join("research-import.json"), &install)?;
    }
    Ok(())
}
fn export(args: &[String]) -> Result<(), String> {
    validate_profile(args)?;
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
        copy_metadata(&game, &staging)?;
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
        let (version, version_source) = if let Some(version) = arg(args, "--version") {
            (Some(version), Some("command line --version".into()))
        } else {
            game_version(&game).map_or((None, None), |(v, s)| (Some(v), Some(s)))
        };
        let install = Installation {
            version,
            version_source,
            executable_version: arg(args, "--exe-version"),
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
    let sources = data.write_review_sources(root)?;
    println!(
        "Imported technology definitions: {}",
        data.registry.technologies.len()
    );
    println!("Country histories: {}", data.histories.len());
    println!("Diagnostics: {}", data.diagnostics.len());
    println!("Report: {}", root.join("research-report.json").display());
    println!("Review source texts: {}", sources.display());
    println!("No original-engine timing comparisons have been performed.");
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reimport_preserves_profile_and_separates_legacy_exe_version() {
        let root =
            std::env::temp_dir().join(format!("hoi4-reimport-profile-{}", std::process::id()));
        fs::create_dir_all(root.join("common/technologies")).unwrap();
        let original = Installation {
            version: Some("1.0".into()),
            scenario: Some("1939".into()),
            enabled_dlcs: vec!["Authored DLC".into()],
            dlc_selection_verified: true,
            data_layers: vec!["layers/one".into()],
            ..Default::default()
        };
        atomic_json(&root.join("research-import.json"), &original).unwrap();
        fs::write(
            root.join("military_profile.txt"),
            "custom_setting=yes\ndata_layers={\"layers/one\"}\n",
        )
        .unwrap();
        refresh_metadata(&root, &[]).unwrap();
        let install = installation(&root);
        assert_eq!(install.scenario, original.scenario);
        assert_eq!(install.enabled_dlcs, original.enabled_dlcs);
        assert_eq!(install.data_layers, original.data_layers);
        assert!(install.dlc_selection_verified);
        assert!(install.version.is_none());
        assert_eq!(install.executable_version.as_deref(), Some("1.0"));
        fs::write(
            root.join("launcher-settings.json"),
            "{\"rawVersion\":\"1.test\"}",
        )
        .unwrap();
        refresh_metadata(&root, &["--no-dlc".into()]).unwrap();
        let install = installation(&root);
        assert_eq!(install.version.as_deref(), Some("1.test"));
        assert!(install.enabled_dlcs.is_empty());
        assert!(install.dlc_selection_verified);
        let profile = fs::read_to_string(root.join("military_profile.txt")).unwrap();
        assert!(profile.contains("custom_setting=yes"));
        let b = hoi4_mobile::pdx::parse_pdx(&profile).unwrap();
        assert!(hoi4_mobile::pdx::block(&b, "dlcs").unwrap().is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    let result=match args.first().map(String::as_str){
        Some("export")=>export(&args[1..]),
        Some("import"|"audit")=>{
            let root=Path::new(args.get(1).map(String::as_str).unwrap_or("data/hoi4"));
            refresh_metadata(root,&args[2.min(args.len())..]).and_then(|_|compile(root))
        },
        _=>Err("Usage: hoi4-research export --game <folder> [--out data/hoi4] [--dlc <exact name> | --no-dlc] [--layer <directory>] | import [data/hoi4] | audit [data/hoi4] [--game <folder>] [--dlc <exact name> | --no-dlc]".into())
    };
    if let Err(e) = result {
        eprintln!("Research import failed: {e}");
        std::process::exit(1);
    }
}
