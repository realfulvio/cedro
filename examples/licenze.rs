//! Fa il file «Licenze di terze parti»: legge `Cargo.lock`, cerca ogni libreria tra quelle scaricate da cargo
//! (o in `vendor/`) e scrive autori, licenza, sito e testo delle licenze, più il file `docs/licenze-manuali.txt`.
//!
//! `cargo run --example licenze -- "Licenze di terze parti.txt"`

use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

struct Package {
    name: String,
    version: String,
    authors: String,
    license: String,
    repository: String,
    texts: Vec<(String, String)>,
}

/// Dalle righe `[package]` di un Cargo.toml: valori semplici e liste (anche su più righe).
fn manifest(text: &str) -> HashMap<String, String> {
    let mut out = HashMap::new();
    let mut in_package = false;
    let mut list: Option<(String, Vec<String>)> = None;
    for line in text.lines() {
        let line = line.trim();
        if let Some((key, items)) = list.as_mut() {
            if line.starts_with(']') {
                out.insert(key.clone(), items.join(", "));
                list = None;
            } else {
                items.push(line.trim_end_matches(',').trim_matches('"').to_string());
            }
            continue;
        }
        if line.starts_with('[') {
            in_package = line == "[package]";
            continue;
        }
        if !in_package {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else { continue };
        let (key, value) = (key.trim(), value.trim());
        if value == "[" {
            list = Some((key.to_string(), Vec::new()));
        } else if let Some(inner) = value.strip_prefix('[').and_then(|v| v.strip_suffix(']')) {
            let joined = inner.split(',').map(|s| s.trim().trim_matches('"')).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(", ");
            out.insert(key.to_string(), joined);
        } else if value.starts_with('"') && !value.starts_with("\"\"\"") {
            out.insert(key.to_string(), value.trim_matches('"').to_string());
        }
    }
    out
}

fn lock_packages(lock: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for block in lock.split("[[package]]").skip(1) {
        let field = |k: &str| block.lines().find_map(|l| l.strip_prefix(&format!("{k} = \"")).map(|v| v.trim_end_matches('"').to_string()));
        if let (Some(name), Some(version)) = (field("name"), field("version")) {
            out.push((name, version));
        }
    }
    out
}

fn registries() -> Vec<PathBuf> {
    let home = std::env::var_os("CARGO_HOME").map(PathBuf::from).or_else(|| {
        std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")).map(|h| PathBuf::from(h).join(".cargo"))
    });
    let mut out = Vec::new();
    if let Some(src) = home.map(|h| h.join("registry").join("src")) {
        if let Ok(dir) = std::fs::read_dir(src) {
            out.extend(dir.flatten().map(|e| e.path()));
        }
    }
    out
}

fn license_files(dir: &Path) -> Vec<(String, String)> {
    let mut files: Vec<(String, String)> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|e| {
            let name = e.file_name().to_string_lossy().into_owned();
            let upper = name.to_uppercase();
            if !(upper.starts_with("LICENSE") || upper.starts_with("LICENCE") || upper.starts_with("COPYING") || upper.starts_with("UNLICENSE") || upper.starts_with("NOTICE")) {
                return None;
            }
            let text = std::fs::read_to_string(e.path()).ok()?;
            Some((name, text.replace("\r\n", "\n")))
        })
        .collect();
    files.sort();
    files
}

fn find(name: &str, version: &str, registries: &[PathBuf]) -> Option<PathBuf> {
    let vendor = Path::new("vendor").join(name);
    if vendor.join("Cargo.toml").exists() {
        return Some(vendor);
    }
    registries.iter().map(|r| r.join(format!("{name}-{version}"))).find(|p| p.join("Cargo.toml").exists())
}

fn main() {
    let out_path = std::env::args().nth(1).unwrap_or_else(|| "Licenze di terze parti.txt".into());
    let lock = std::fs::read_to_string("Cargo.lock").expect("Cargo.lock non trovato: lancia dalla cartella di Cedro");
    let registries = registries();
    let mut packages = Vec::new();
    let mut missing = Vec::new();
    for (name, version) in lock_packages(&lock) {
        if name == "cedro" {
            continue;
        }
        let Some(dir) = find(&name, &version, &registries) else {
            missing.push(format!("{name} {version}"));
            packages.push(Package { name, version, authors: String::new(), license: String::new(), repository: String::new(), texts: Vec::new() });
            continue;
        };
        let toml = manifest(&std::fs::read_to_string(dir.join("Cargo.toml")).unwrap_or_default());
        let get = |k: &str| toml.get(k).cloned().unwrap_or_default();
        let license = if get("license").is_empty() { get("license-file") } else { get("license") };
        packages.push(Package {
            name,
            version,
            authors: get("authors"),
            license,
            repository: get("repository"),
            texts: license_files(&dir),
        });
    }
    packages.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()).then(a.version.cmp(&b.version)));

    let mut out = String::new();
    let _ = writeln!(out, "LICENZE DI TERZE PARTI - Cedro {}", env!("CARGO_PKG_VERSION"));
    let _ = writeln!(out, "Cedro è software libero (licenza MIT). Usa il lavoro di queste persone: grazie.");
    let _ = writeln!(out, "Le librerie sono {}; i testi uguali compaiono una volta sola.\n", packages.len());
    out += &std::fs::read_to_string("docs/licenze-manuali.txt").unwrap_or_default().replace("\r\n", "\n");
    out += "\nLIBRERIE RUST\n=============\n";

    let mut seen: HashMap<String, String> = HashMap::new();
    for p in &packages {
        let _ = writeln!(out, "\n--- {} {} ---", p.name, p.version);
        if !p.authors.is_empty() {
            let _ = writeln!(out, "Autori: {}", p.authors);
        }
        let _ = writeln!(out, "Licenza: {}", if p.license.is_empty() { "vedi il progetto" } else { &p.license });
        if !p.repository.is_empty() {
            let _ = writeln!(out, "Sito: {}", p.repository);
        }
        for (file, text) in &p.texts {
            let key = text.split_whitespace().collect::<Vec<_>>().join(" ");
            match seen.get(&key) {
                Some(first) => {
                    let _ = writeln!(out, "\n[{file}: stesso testo di {first}]");
                }
                None => {
                    seen.insert(key, format!("{} {}", p.name, p.version));
                    let _ = writeln!(out, "\n[{file}]\n{}", text.trim_end());
                }
            }
        }
    }
    std::fs::write(&out_path, out).expect("scrittura del file");
    println!("{} librerie, {} KB in {out_path}", packages.len(), std::fs::metadata(&out_path).map(|m| m.len() / 1024).unwrap_or(0));
    if !missing.is_empty() {
        eprintln!("non trovate ({}): {}", missing.len(), missing.join(", "));
    }
}
