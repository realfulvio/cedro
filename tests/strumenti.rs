//! I profili degli strumenti contro i file d'esempio pubblici di allotropy (Benchling, licenza MIT).
//!
//! Per ogni file d'esempio sta in `tests/strumenti/golden/<profilo>/` un riassunto di quello che
//! allotropy ne ricava (strumento, e per ogni misura pozzetto, campione e valori). La prova legge il
//! file con Cedro e confronta. I file d'esempio non stanno nel repository (qualche decina di MB):
//! si prova con `CEDRO_ALLOTROPY=/percorso/del/clone/di/allotropy cargo test --release --test strumenti`.
//! Senza la variabile le prove sui file d'esempio si saltano e restano quelle sui file piccoli.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use cedro::strumenti::{self, asm, excel, Fonte, Lettura, Misura};
use serde_json::Value;

fn golden_dir() -> PathBuf {
    std::env::var_os("CEDRO_GOLDEN").map(PathBuf::from).unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/strumenti/golden"))
}

fn allotropy() -> Option<PathBuf> {
    std::env::var_os("CEDRO_ALLOTROPY").map(PathBuf::from).filter(|p| p.join("tests/parsers").is_dir())
}

fn uguali(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 * a.abs().max(b.abs()).max(1.0) * 1e-3 + 1e-12 || (a - b).abs() <= 1e-9 * a.abs().max(b.abs())
}

fn tenta(m: &Misura, vals: &serde_json::Map<String, Value>) -> Result<(), String> {
    for (k, v) in vals {
        let Some(x) = v.as_f64() else { continue };
        let ok = m.valori.iter().any(|c| c.chiave == *k && c.numero().is_some_and(|y| uguali(x, y)));
        if !ok {
            let visto: Vec<String> = m.valori.iter().filter(|c| c.chiave == *k).map(|c| format!("{:?}", c.dato)).collect();
            return Err(format!("{k}: atteso {x}, letto {}", if visto.is_empty() { "niente".into() } else { visto.join(" / ") }));
        }
    }
    Ok(())
}

/// Confronta la lettura con il riassunto di allotropy. Ritorna le differenze.
fn confronta(l: &Lettura, g: &Value) -> Vec<String> {
    let mut diff = Vec::new();
    // Lo strumento.
    if let Some(s) = g["strumento"].as_object() {
        let prova = |k: &str, letto: &str, diff: &mut Vec<String>| {
            if let Some(a) = s.get(k).and_then(Value::as_str).filter(|a| *a != "N/A" && !a.is_empty()) {
                if a != letto {
                    diff.push(format!("{k}: atteso «{a}», letto «{letto}»"));
                }
            }
        };
        prova("equipment serial number", &l.strumento.seriale, &mut diff);
        prova("product manufacturer", &l.strumento.produttore, &mut diff);
        let id = if l.strumento.identificativo.is_empty() { &l.strumento.modello } else { &l.strumento.identificativo };
        prova("device identifier", id, &mut diff);
        prova("model number", &l.strumento.modello, &mut diff);
    }
    let attese = g["misure"].as_array().cloned().unwrap_or_default();
    if l.conta() != attese.len() {
        diff.push(format!("misure: attese {}, lette {}", attese.len(), l.conta()));
    }
    // Ogni misura attesa deve trovare una misura letta con lo stesso pozzetto, campione e valori.
    let lette: Vec<&Misura> = l.misure().collect();
    let mut usate: HashSet<usize> = HashSet::new();
    let mut errori = 0;
    for a in &attese {
        let loc = a["loc"].as_str();
        let campione = a["sample"].as_str();
        let vals = a["vals"].as_object().cloned().unwrap_or_default();
        let mut ultimo: Option<String> = None;
        let trovata = lette.iter().enumerate().find(|(i, m)| {
            if usate.contains(i) {
                return false;
            }
            if loc.is_some() && m.pozzetto.as_deref() != loc {
                return false;
            }
            if campione.is_some() && !m.campione.is_empty() && Some(m.campione.as_str()) != campione {
                return false;
            }
            match tenta(m, &vals) {
                Ok(()) => true,
                Err(e) => {
                    ultimo = Some(e);
                    false
                }
            }
        });
        match trovata {
            Some((i, _)) => {
                usate.insert(i);
            }
            None => {
                errori += 1;
                if errori <= 4 {
                    diff.push(format!("misura {:?} / {:?}: {}", loc, campione, ultimo.unwrap_or_else(|| "nessuna con questo pozzetto e campione".into())));
                }
            }
        }
    }
    if errori > 4 {
        diff.push(format!("… e altre {} misure diverse", errori - 4));
    }
    diff
}

fn riassunti(profilo: &str) -> Vec<(PathBuf, Value)> {
    let dir = golden_dir().join(profilo);
    let Ok(rd) = std::fs::read_dir(&dir) else { return Vec::new() };
    let mut v: Vec<(PathBuf, Value)> = rd.flatten().map(|e| e.path()).filter(|p| p.to_string_lossy().ends_with(".golden.json")).filter_map(|p| serde_json::from_slice(&std::fs::read(&p).ok()?).ok().map(|j| (p, j))).collect();
    v.sort_by(|a, b| a.0.cmp(&b.0));
    v
}

fn ingresso(root: &Path, profilo: &str, g: &Value) -> PathBuf {
    let rel = g["file"].as_str().unwrap_or_default().replace("__", "/");
    root.join("tests/parsers").join(profilo).join("testdata").join(rel)
}

/// Tutti i profili pronti leggono giusti i loro file d'esempio.
#[test]
fn i_profili_leggono_i_file_di_allotropy() {
    let Some(root) = allotropy() else {
        eprintln!("CEDRO_ALLOTROPY non impostata: prova saltata");
        return;
    };
    let solo = std::env::var("CEDRO_PROFILO").ok().filter(|s| !s.is_empty());
    let mut falliti: Vec<String> = Vec::new();
    let mut letti = 0usize;
    for p in strumenti::profili() {
        if solo.as_deref().is_some_and(|s| s != p.id) {
            continue;
        }
        let r = riassunti(p.id);
        assert!(!r.is_empty(), "{}: nessun riassunto in {}", p.id, golden_dir().display());
        for (_, g) in r {
            let file = ingresso(&root, p.id, &g);
            let nome = g["file"].as_str().unwrap_or_default().to_string();
            if !file.exists() {
                falliti.push(format!("{} · {}: file d'esempio assente", p.id, nome));
                continue;
            }
            let fonte = match Fonte::apri(&file) {
                Ok(f) => f,
                Err(e) => {
                    falliti.push(format!("{} · {}: {e}", p.id, nome));
                    continue;
                }
            };
            // Il profilo giusto deve essere quello che vince.
            match strumenti::riconosci(&fonte) {
                Some((q, _)) if q.id == p.id => {}
                Some((q, s)) => falliti.push(format!("{} · {}: riconosciuto come {} ({s})", p.id, nome, q.id)),
                None => falliti.push(format!("{} · {}: non riconosciuto", p.id, nome)),
            }
            match strumenti::leggi_con(p, &fonte) {
                Ok(l) => {
                    letti += 1;
                    let d = confronta(&l, &g);
                    if !d.is_empty() {
                        falliti.push(format!("{} · {}:\n    {}", p.id, nome, d.join("\n    ")));
                    }
                }
                Err(e) => falliti.push(format!("{} · {}: {e}", p.id, nome)),
            }
        }
    }
    eprintln!("file letti: {letti}");
    assert!(falliti.is_empty(), "{} file diversi da allotropy:\n{}", falliti.len(), falliti.join("\n"));
}

/// L'Excel e il CSV piatto si scrivono per ogni file, e l'Allotrope passa il controllo leggero.
#[test]
fn le_uscite_si_scrivono() {
    let Some(root) = allotropy() else { return };
    let mut falliti: Vec<String> = Vec::new();
    for p in strumenti::profili() {
        for (_, g) in riassunti(p.id) {
            let file = ingresso(&root, p.id, &g);
            let Ok(fonte) = Fonte::apri(&file) else { continue };
            let Ok(l) = strumenti::leggi_con(p, &fonte) else { continue };
            let nome = g["file"].as_str().unwrap_or_default();
            if let Err(e) = excel::scrivi(&l, &excel::Opzioni::default()) {
                falliti.push(format!("{} · {nome}: Excel: {e}", p.id));
            }
            let o = asm::Opzioni { calcoli: true, ..Default::default() };
            let v = asm::scrivi(&l, &o);
            if let Some(dir) = std::env::var_os("CEDRO_ASM_OUT") {
                let dir = PathBuf::from(dir);
                let _ = std::fs::create_dir_all(&dir);
                let _ = std::fs::write(dir.join(format!("{}__{}.json", p.id, nome.replace('/', "__").replace("__", "_"))), serde_json::to_vec_pretty(&v).unwrap());
            }
            if let Err(e) = asm::controlla(&v) {
                falliti.push(format!("{} · {nome}: Allotrope: {e}", p.id));
            }
            if asm::piatto(&l, &o).lines().count() != l.conta() + 1 {
                falliti.push(format!("{} · {nome}: il CSV piatto non ha una riga per misura", p.id));
            }
        }
    }
    assert!(falliti.is_empty(), "{}", falliti.join("\n"));
}
