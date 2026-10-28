//! Convertire il contenuto di un archivio ZIP: i file del tipo scelto (foto, audio, documenti) si
//! convertono, gli altri restano com'erano, e l'archivio si rifà con la stessa struttura di cartelle.
//! Se qualche file non si converte resta l'originale e un «RAPPORTO-CEDRO.txt» dentro lo ZIP dice perché.

use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};

use crate::engine::{self, Kind, Options};

/// Cosa converte una voce dell'archivio: i file di quel tipo e il formato di uscita.
fn target(action: &str) -> Option<(&'static [Kind], &'static str)> {
    match action {
        "z-webp" => Some((&[Kind::Image], "webp")),
        "z-jpeg" => Some((&[Kind::Image], "jpeg")),
        "z-png" => Some((&[Kind::Image], "png")),
        "z-mp3" => Some((&[Kind::Audio], "mp3")),
        "z-wav" => Some((&[Kind::Audio], "wav")),
        "z-pdf" => Some((&[Kind::Doc], "pdf")),
        _ => None,
    }
}

/// Il numero massimo di file e di byte che si accettano in un archivio (contro gli archivi «bomba»).
const MAX_FILES: usize = 20_000;
const MAX_BYTES: u64 = 8 * 1024 * 1024 * 1024;

fn is_media_already_packed(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    [".png", ".jpg", ".jpeg", ".webp", ".avif", ".mp3", ".m4a", ".ogg", ".opus", ".flac", ".mp4", ".mkv", ".webm", ".zip", ".docx", ".odt", ".pdf"].iter().any(|e| n.ends_with(e))
}

/// Tutti i file sotto `root`, come percorsi relativi.
fn walk(root: &Path, base: &Path, out: &mut Vec<PathBuf>) {
    let Ok(read) = fs::read_dir(base) else { return };
    let mut entries: Vec<_> = read.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        if p.is_dir() {
            walk(root, &p, out);
        } else if let Ok(rel) = p.strip_prefix(root) {
            out.push(rel.to_path_buf());
        }
    }
}

fn entry_name(rel: &Path) -> String {
    rel.components()
        .filter_map(|c| match c {
            Component::Normal(s) => Some(s.to_string_lossy().into_owned()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Converte lo ZIP `path` secondo `action` (`z-webp`, `z-mp3`…) e scrive il nuovo ZIP in `dir`.
pub fn convert(path: &Path, action: &str, quality: u8, dir: &Path, opts: &Options) -> Result<(PathBuf, String), String> {
    let (kinds, format) = target(action).ok_or_else(|| format!("azione sconosciuta: {action}"))?;
    let mut zip = zip::ZipArchive::new(fs::File::open(path).map_err(|e| e.to_string())?).map_err(|e| format!("archivio non letto: {e}"))?;
    if zip.len() > MAX_FILES {
        return Err(format!("l'archivio ha troppi file ({}): il limite è {MAX_FILES}", zip.len()));
    }
    let declared: u64 = (0..zip.len()).filter_map(|i| zip.by_index_raw(i).ok().map(|e| e.size())).sum();
    if declared > MAX_BYTES {
        return Err("l'archivio contiene più di 8 GB di file: troppo grande".into());
    }

    let work = engine::private_temp_dir("cedro-zip")?;
    let result = convert_in(&mut zip, path, action, kinds, format, quality, dir, opts, &work);
    let _ = fs::remove_dir_all(&work);
    result
}

#[allow(clippy::too_many_arguments)]
fn convert_in(
    zip: &mut zip::ZipArchive<fs::File>,
    source: &Path,
    action: &str,
    kinds: &[Kind],
    format: &str,
    quality: u8,
    dir: &Path,
    opts: &Options,
    work: &Path,
) -> Result<(PathBuf, String), String> {
    let (inside, outside) = (work.join("in"), work.join("out"));
    let mut files: Vec<PathBuf> = Vec::new();
    let mut ignored = 0usize;
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| format!("archivio non letto: {e}"))?;
        if entry.is_dir() {
            continue;
        }
        // I percorsi che escono dalla cartella («../») non si estraggono.
        let Some(rel) = entry.enclosed_name() else {
            ignored += 1;
            continue;
        };
        let dest = inside.join(&rel);
        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = fs::File::create(&dest).map_err(|e| e.to_string())?;
        let size = entry.size();
        std::io::copy(&mut (&mut entry).take(size), &mut out).map_err(|e| format!("«{}» non estratto: {e}", rel.display()))?;
        files.push(rel);
    }
    if files.is_empty() {
        return Err("l'archivio è vuoto".into());
    }

    let inner = Options { exists: engine::Exists::Number, on_exists: None, name_template: None, ..opts.clone() };
    let (mut converted, mut kept) = (0usize, 0usize);
    let mut failures: Vec<(String, String)> = Vec::new();
    let total = files.len();
    // Prima si copiano i file che restano com'erano: così i loro nomi sono già presi e una conversione
    // (per esempio «foto.png» in WebP) non può sovrascrivere un «foto.webp» che c'è nello ZIP, in qualunque ordine stiano.
    let mut to_convert: Vec<&PathBuf> = Vec::new();
    for rel in &files {
        let src = inside.join(rel);
        let out_dir = outside.join(rel.parent().unwrap_or(Path::new("")));
        fs::create_dir_all(&out_dir).map_err(|e| e.to_string())?;
        let ext = engine::extension(&src);
        let already = match format {
            "jpeg" => matches!(ext.as_str(), "jpg" | "jpeg"),
            other => ext == other,
        };
        if kinds.contains(&engine::kind_of(&src)) && !already {
            to_convert.push(rel);
        } else {
            fs::copy(&src, out_dir.join(src.file_name().unwrap_or_default())).map_err(|e| e.to_string())?;
            kept += 1;
        }
    }
    for (i, rel) in to_convert.into_iter().enumerate() {
        crate::progress::report(i as f32 / total as f32);
        let src = inside.join(rel);
        let out_dir = outside.join(rel.parent().unwrap_or(Path::new("")));
        match engine::perform_with(&src, format, quality, Some(&out_dir), &inner) {
            Ok(_) => converted += 1,
            Err(e) => {
                failures.push((entry_name(rel), e.0));
                // L'originale resta: il suo nome ha l'estensione di partenza, diversa da quella di uscita.
                fs::copy(&src, out_dir.join(src.file_name().unwrap_or_default())).map_err(|e| e.to_string())?;
                kept += 1;
            }
        }
    }
    if converted == 0 && failures.is_empty() {
        return Err(match kinds[0] {
            Kind::Image => "nello ZIP non ci sono foto da convertire in questo formato".into(),
            Kind::Audio => "nello ZIP non c'è audio da convertire in questo formato".into(),
            _ => "nello ZIP non ci sono documenti da convertire in questo formato".into(),
        });
    }

    // Lo ZIP nuovo: stessa struttura di cartelle, file già compressi senza ricomprimerli.
    let name = format!("{}-{}", source.file_stem().and_then(|s| s.to_str()).unwrap_or("archivio"), action.trim_start_matches("z-"));
    let out_path = engine::unique_output(dir, &name, "zip");
    let mut written: Vec<PathBuf> = Vec::new();
    walk(&outside, &outside, &mut written);
    let file = fs::File::create(&out_path).map_err(|e| e.to_string())?;
    let mut writer = zip::ZipWriter::new(file);
    let pack = |name: &str| {
        let method = if is_media_already_packed(name) { zip::CompressionMethod::Stored } else { zip::CompressionMethod::Deflated };
        zip::write::SimpleFileOptions::default().compression_method(method)
    };
    let build = (|| -> Result<(), String> {
        for rel in &written {
            let name = entry_name(rel);
            writer.start_file(&name, pack(&name)).map_err(|e| e.to_string())?;
            let mut src = fs::File::open(outside.join(rel)).map_err(|e| e.to_string())?;
            std::io::copy(&mut src, &mut writer).map_err(|e| e.to_string())?;
        }
        if !failures.is_empty() {
            let mut report = String::from("File che Cedro non ha potuto convertire (nello ZIP c'è l'originale):\r\n\r\n");
            for (name, why) in &failures {
                report.push_str(&format!("- {name}: {why}\r\n"));
            }
            writer.start_file("RAPPORTO-CEDRO.txt", zip::write::SimpleFileOptions::default()).map_err(|e| e.to_string())?;
            writer.write_all(report.as_bytes()).map_err(|e| e.to_string())?;
        }
        writer.finish().map(|_| ()).map_err(|e| e.to_string())
    })();
    if let Err(e) = build {
        let _ = fs::remove_file(&out_path);
        return Err(e);
    }

    // Una frase per ogni cosa, separate da «. »: così l'inglese le traduce una per una.
    let mut parts = vec![if converted == 1 { "1 file convertito".to_string() } else { format!("{converted} file convertiti") }];
    if kept > failures.len() {
        parts.push(format!("{} lasciati com'erano", kept - failures.len()));
    }
    if !failures.is_empty() {
        parts.push(format!("{} non convertiti (vedi RAPPORTO-CEDRO.txt)", failures.len()));
    }
    if ignored > 0 {
        parts.push(format!("{ignored} con percorso non sicuro ignorati"));
    }
    let message = parts.join(". ");
    Ok((out_path, message))
}
