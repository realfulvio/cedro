//! Fa il file «Licenze di terze parti»: chiede a cargo le librerie davvero usate per la piattaforma
//! (autori, licenza, sito), ne legge i testi delle licenze dai sorgenti e aggiunge `docs/licenze-manuali.txt`.
//!
//! `cargo run --release --example licenze -- "Licenze di terze parti.txt" [triple]`
//! Senza triple usa quello di questo PC (su Windows `x86_64-pc-windows-msvc`).

use std::collections::{HashMap, HashSet};
use std::fmt::Write as _;
use std::path::Path;
use std::process::Command;

struct Package {
    name: String,
    version: String,
    authors: String,
    license: String,
    repository: String,
    texts: Vec<(String, String)>,
}

fn host_triple() -> String {
    let out = Command::new("rustc").arg("-vV").output().expect("rustc non trovato");
    String::from_utf8_lossy(&out.stdout)
        .lines()
        .find_map(|l| l.strip_prefix("host: "))
        .unwrap_or("x86_64-pc-windows-msvc")
        .to_string()
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

fn main() {
    let out_path = std::env::args().nth(1).unwrap_or_else(|| "Licenze di terze parti.txt".into());
    let triple = std::env::args().nth(2).unwrap_or_else(host_triple);
    let meta = Command::new(std::env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--format-version", "1", "--filter-platform", &triple])
        .output()
        .expect("cargo metadata non è partito");
    assert!(meta.status.success(), "cargo metadata: {}", String::from_utf8_lossy(&meta.stderr));
    let meta: serde_json::Value = serde_json::from_slice(&meta.stdout).expect("risposta di cargo metadata");

    // Solo le librerie che entrano davvero nel programma per questa piattaforma.
    let used: HashSet<&str> = meta["resolve"]["nodes"].as_array().into_iter().flatten().filter_map(|n| n["id"].as_str()).collect();
    let text = |v: &serde_json::Value| v.as_str().unwrap_or("").to_string();
    let mut packages = Vec::new();
    for p in meta["packages"].as_array().into_iter().flatten() {
        let name = text(&p["name"]);
        if name == "cedro" || !used.contains(p["id"].as_str().unwrap_or("")) {
            continue;
        }
        let authors = p["authors"].as_array().into_iter().flatten().filter_map(|a| a.as_str()).collect::<Vec<_>>().join(", ");
        let dir = Path::new(p["manifest_path"].as_str().unwrap_or("")).parent().map(Path::to_path_buf).unwrap_or_default();
        let license = if p["license"].is_null() { text(&p["license_file"]) } else { text(&p["license"]) };
        packages.push(Package { name, version: text(&p["version"]), authors, license, repository: text(&p["repository"]), texts: license_files(&dir) });
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
        if p.texts.is_empty() {
            let _ = writeln!(out, "\n[Il pacchetto non contiene il testo della licenza: è quello standard della licenza indicata, sul sito del progetto.]");
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
    println!("{} librerie per {triple}, {} KB in {out_path}", packages.len(), std::fs::metadata(&out_path).map(|m| m.len() / 1024).unwrap_or(0));
    let without: Vec<String> = packages.iter().filter(|p| p.texts.is_empty()).map(|p| format!("{} {}", p.name, p.version)).collect();
    if !without.is_empty() {
        eprintln!("senza file di licenza ({}): {}", without.len(), without.join(", "));
    }
}
