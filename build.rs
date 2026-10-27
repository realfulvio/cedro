// Su Windows incorpora l'icona e i dati di versione nell'exe.
// Su ogni piattaforma passa a Cedro le versioni delle librerie (da Cargo.lock) per la scheda Informazioni.
fn main() {
    println!("cargo:rustc-env=CEDRO_CRATES={}", crate_versions());
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=trascrivi/Cargo.lock");
    #[cfg(windows)]
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut res = winresource::WindowsResource::new();
        res.set_icon(if std::env::var_os("CARGO_FEATURE_BONDENO").is_some() { "assets/cedro-bondeno.ico" } else { "assets/cedro.ico" });
        res.set("FileDescription", "Cedro");
        res.set("ProductName", "Cedro");
        // Controlli con lo stile attuale di Windows nella finestra delle impostazioni.
        res.set_manifest(
            r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
<dependency><dependentAssembly><assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls" version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/></dependentAssembly></dependency>
<compatibility xmlns="urn:schemas-microsoft-com:compatibility.v1"><application><supportedOS Id="{8e0f7a12-bfb3-4fe8-b9a5-48fd50a15a9a}"/></application></compatibility>
</assembly>"#,
        );
        if let Err(e) = res.compile() {
            println!("cargo:warning=icona non incorporata: {e}");
        }
    }
    println!("cargo:rerun-if-changed=assets/cedro.ico");
    println!("cargo:rerun-if-changed=assets/cedro-bondeno.ico");
    println!("cargo:rerun-if-changed=build.rs");
}

/// «nome=versione» per ogni libreria di Cargo.lock (e di quello del motore), separate da «;».
/// Con più versioni della stessa libreria vince quella che Cedro chiede in Cargo.toml, altrimenti la più alta.
fn crate_versions() -> String {
    let wanted = declared("Cargo.toml");
    let mut found: Vec<(String, String)> = Vec::new();
    for lock in ["Cargo.lock", "trascrivi/Cargo.lock"] {
        let text = std::fs::read_to_string(lock).unwrap_or_default();
        let mut name = String::new();
        for line in text.lines() {
            if let Some(n) = line.strip_prefix("name = \"") {
                name = n.trim_end_matches('"').to_string();
            } else if let Some(v) = line.strip_prefix("version = \"") {
                if !name.is_empty() {
                    let entry = (name.clone(), v.trim_end_matches('"').to_string());
                    if lock == "Cargo.lock" || !found.iter().any(|(n, _)| *n == entry.0) {
                        found.push(entry);
                    }
                    name.clear();
                }
            }
        }
    }
    let key = |v: &str| -> Vec<u64> { v.split(|c: char| !c.is_ascii_digit()).filter_map(|p| p.parse().ok()).collect() };
    let mut names: Vec<&String> = found.iter().map(|(n, _)| n).collect();
    names.sort();
    names.dedup();
    let mut out = Vec::new();
    for n in names {
        let mut versions: Vec<&String> = found.iter().filter(|(m, _)| m == n).map(|(_, v)| v).collect();
        versions.sort_by_key(|v| key(v));
        let asked = wanted.iter().find(|(m, _)| m == n).map(|(_, v)| key(v));
        let pick = asked
            .and_then(|a| versions.iter().rev().find(|v| key(v).iter().zip(a.iter()).all(|(x, y)| x == y)).copied())
            .or(versions.last().copied());
        if let Some(v) = pick {
            out.push(format!("{n}={v}"));
        }
    }
    out.join(";")
}

/// Le librerie che Cargo.toml chiede, con la versione scritta («0.25», «2»).
fn declared(file: &str) -> Vec<(String, String)> {
    let text = std::fs::read_to_string(file).unwrap_or_default();
    let mut out = Vec::new();
    for line in text.lines() {
        let Some((name, rest)) = line.split_once(" = ") else { continue };
        let name = name.trim();
        if name.is_empty() || name.contains(' ') || name.starts_with('[') {
            continue;
        }
        let version = if let Some(v) = rest.trim().strip_prefix('"') {
            v.split('"').next()
        } else {
            rest.split("version = \"").nth(1).and_then(|v| v.split('"').next())
        };
        if let Some(v) = version {
            out.push((name.to_string(), v.trim_start_matches(['^', '=', '~']).to_string()));
        }
    }
    out
}
