//! I profili degli strumenti contro i file d'esempio pubblici di allotropy (Benchling, licenza MIT).
//!
//! Per ogni file d'esempio sta in `tests/strumenti/golden/<profilo>/` un riassunto di quello che
//! allotropy ne ricava (strumento, e per ogni misura pozzetto, campione e valori). La prova legge il
//! file con Cedro e confronta. I file d'esempio non stanno nel repository (qualche decina di MB):
//! si prova con `CEDRO_ALLOTROPY=/percorso/del/clone/di/allotropy cargo test --release --test strumenti`.
//! Senza la variabile le prove sui file d'esempio si saltano e restano quelle sui file piccoli.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use cedro::strumenti::{self, asm, excel, Dato, Fonte, Lettura, Misura};
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
        // «-0» è il posto del dato che il file non ha: Cedro lo tiene come assente.
        let manca = x == 0.0 && x.is_sign_negative();
        let ok = m.valori.iter().any(|c| c.chiave == *k && (c.numero().is_some_and(|y| uguali(x, y)) || (manca && matches!(c.dato, Dato::Assente(_)))));
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
                // allotropy scrive «Nanodrop 8000»: Cedro il nome giusto («NanoDrop 8000»).
                // «s/n 9000…» e «9000…» sono lo stesso numero di serie.
                let senza = |x: &str| -> String {
                    let t = x.trim();
                    let t = if t.get(..3).is_some_and(|p| p.eq_ignore_ascii_case("s/n")) { &t[3..] } else { t };
                    t.trim_start_matches([':', ' ']).to_string()
                };
                // «ThermoFisher Scientific» e «Thermo Fisher Scientific» sono lo stesso produttore.
                let senza_spazi = |x: &str| x.split_whitespace().collect::<String>();
                if !a.eq_ignore_ascii_case(letto) && !(k == "equipment serial number" && senza(a).eq_ignore_ascii_case(&senza(letto))) && !(k == "product manufacturer" && senza_spazi(a).eq_ignore_ascii_case(&senza_spazi(letto))) {
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
    // Le misure di allotropy senza valori (lo spettro intero, che Cedro tiene a parte) non si confrontano;
    // le altre devono esserci tutte. Cedro può averne di più (le assorbanze a zero, che allotropy scarta).
    let attese: Vec<Value> = g["misure"].as_array().cloned().unwrap_or_default().into_iter().filter(|a| a["vals"].as_object().is_some_and(|v| !v.is_empty()) || a["spettro"].as_u64().is_some()).collect();
    if l.misure().count() < attese.len() {
        diff.push(format!("misure: attese almeno {}, lette {}", attese.len(), l.misure().count()));
    }
    // Ogni misura attesa deve trovare una misura letta con lo stesso pozzetto, campione e valori.
    let lette: Vec<&Misura> = l.misure().collect();
    let mut usate: HashSet<usize> = HashSet::new();
    let mut errori = 0;
    for a in &attese {
        // «nan» è il posto mancante di allotropy (pandas).
        let loc = a["loc"].as_str().filter(|s| *s != "nan");
        let campione = a["sample"].as_str().filter(|s| *s != "nan");
        let vals = a["vals"].as_object().cloned().unwrap_or_default();
        let mut ultimo: Option<String> = None;
        let trovata = lette.iter().enumerate().find(|(i, m)| {
            if usate.contains(i) {
                return false;
            }
            // «1.0» di allotropy e «1» del file sono lo stesso posto.
            let stesso = |a: Option<&str>, b: Option<&str>| a == b || a.and_then(|x| x.parse::<f64>().ok()).is_some_and(|x| b.and_then(|y| y.parse::<f64>().ok()) == Some(x));
            if loc.is_some() && !stesso(m.pozzetto.as_deref(), loc) && !stesso(m.luogo.as_deref(), loc) {
                return false;
            }
            // Un campione che è un numero («800») per pandas diventa «800.0»: lo stesso campione.
            let stesso_campione = |a: &str, b: &str| a == b || a.parse::<f64>().ok().is_some_and(|x| b.parse::<f64>().ok() == Some(x));
            if campione.is_some_and(|c| !m.campione.is_empty() && !stesso_campione(&m.campione, c)) {
                return false;
            }
            // Lo spettro di assorbanza: i punti devono essere tutti.
            if let Some(n) = a["spettro"].as_u64() {
                if m.spettro.len() as u64 != n {
                    ultimo = Some(format!("spettro: attesi {n} punti, letti {}", m.spettro.len()));
                    return false;
                }
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
    // Un file d'esempio può stare nella cartella di un altro lettore di allotropy («origine»).
    let origine = g["origine"].as_str().unwrap_or(profilo);
    root.join("tests/parsers").join(origine).join("testdata").join(rel)
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
            match excel::scrivi(&l, &excel::Opzioni::default()) {
                Ok(b) => {
                    if let Some(dir) = std::env::var_os("CEDRO_XLSX_OUT") {
                        let dir = PathBuf::from(dir);
                        let _ = std::fs::create_dir_all(&dir);
                        let _ = std::fs::write(dir.join(format!("{}__{}.xlsx", p.id, nome.replace('/', "_"))), b);
                    }
                }
                Err(e) => falliti.push(format!("{} · {nome}: Excel: {e}", p.id)),
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
