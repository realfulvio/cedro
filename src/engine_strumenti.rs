//! L'azione «strumento»: legge il file di uno strumento di laboratorio (lettore di piastre, qPCR,
//! spettrofotometro, contacellule…) e scrive l'Excel con le misure, e a richiesta il CSV, Allotrope
//! (ASM) e il CSV piatto. I nomi dei file nuovi seguono le regole di sempre (data e caso, «-2» se esistono).

use super::*;
use crate::strumenti::{scelte, Fonte};
use std::cell::RefCell;
use std::sync::Mutex;

/// Quello che il lavoro sa dire dopo, oltre al file principale: i file in più, lo strumento, l'originale
/// con la sua impronta e le note per chi ha convertito.
#[derive(Clone, Debug)]
pub struct Esito {
    /// «Tecan Magellan · Sunrise · Lettore di piastre · Piastra 1 · 24/11/2026 14:05».
    pub strumento: String,
    /// Nome e impronta SHA-256 del file letto.
    pub origine: (String, String),
    /// Gli altri file scritti, oltre a quello che il lavoro riporta come uscita.
    pub altri: Vec<PathBuf>,
    pub note: Vec<scelte::Nota>,
}

thread_local! {
    /// L'esito del lavoro in corso su questo thread, in attesa che `perform_with` sappia i nomi definitivi.
    static ULTIMO: RefCell<Option<Esito>> = const { RefCell::new(None) };
}

/// Gli esiti dei lavori finiti, per il file principale che hanno scritto.
static ESITI: Mutex<Vec<(PathBuf, Esito)>> = Mutex::new(Vec::new());

/// L'esito del lavoro sui dati di uno strumento che ha scritto `principale` (si legge una volta sola).
pub fn esito_strumento(principale: &Path) -> Option<Esito> {
    let mut v = ESITI.lock().unwrap_or_else(|e| e.into_inner());
    let i = v.iter().position(|(p, _)| p == principale)?;
    Some(v.remove(i).1)
}

/// Dopo `finish_outputs`: i nomi dei file sono definitivi, l'esito si ricorda per chi lo chiede.
pub(super) fn registra_esito(out: &Output, pending: &[(PathBuf, PathBuf)]) {
    let Some(mut e) = ULTIMO.with(|u| u.borrow_mut().take()) else { return };
    for p in e.altri.iter_mut() {
        if let Some((_, target)) = pending.iter().find(|(tmp, _)| tmp == p) {
            *p = target.clone();
        }
    }
    let mut v = ESITI.lock().unwrap_or_else(|e| e.into_inner());
    // Non si accumulano esiti che nessuno ha chiesto.
    if v.len() > 64 {
        v.drain(..32);
    }
    v.retain(|(p, _)| *p != out.path);
    v.push((out.path.clone(), e));
}

/// Si butta l'esito rimasto (lavoro fallito).
pub(super) fn scarta_esito() {
    ULTIMO.with(|u| u.borrow_mut().take());
}

fn nome_file(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

pub(super) fn scrivi(path: &Path, dir: &Path, opts: &Options) -> CedroResult<Output> {
    let s = &opts.strumento;
    if !s.esce_qualcosa() {
        return Err(err("scegli almeno un'uscita"));
    }
    let fonte = Fonte::apri(path).map_err(err)?;
    crate::progress::report(0.1);
    let l = scelte::leggi_scelto(&fonte, &s.profilo).map_err(err)?;
    crate::progress::report(0.4);
    // Un'uscita con la stessa estensione dell'originale non può avere il suo nome: prende «-pulito».
    let ext_origine = extension(path);
    let base = stem(path);
    let nome_per = |ext: &str, suffisso: &str| -> String {
        let suffisso = if suffisso.is_empty() && ext == ext_origine { "-pulito" } else { suffisso };
        format!("{base}{suffisso}")
    };
    let a_piastra = l.a_piastra();
    // I nomi si scelgono prima di scrivere: se uno va saltato ci si ferma senza aver fatto niente.
    let x_out = if s.excel { Some(reserve(dir, &nome_per("xlsx", ""), "xlsx")?) } else { None };
    let c_out = if s.csv { Some(reserve(dir, &nome_per("csv", if a_piastra { "_pozzetti" } else { "" }), "csv")?) } else { None };
    let a_out = if s.asm { Some(reserve(dir, &nome_per("json", ""), "asm.json")?) } else { None };
    let p_out = if s.piatto { Some(reserve(dir, &nome_per("csv", "_piatto"), "csv")?) } else { None };
    let nome_asm = a_out.as_deref().map(nome_file).unwrap_or_default();
    let origine = path.display().to_string();
    let sep = opts.csv_separator.unwrap_or(';');
    let u = scelte::prepara(&l, s, &origine, &nome_asm, sep).map_err(err)?;
    crate::progress::report(0.8);
    let mut scritti: Vec<PathBuf> = Vec::new();
    let mut parti: Vec<&str> = Vec::new();
    if let (Some(p), Some(b)) = (&x_out, &u.excel) {
        write_bytes(p, b)?;
        scritti.push(p.clone());
        parti.push("Excel");
    }
    if let (Some(p), Some(t)) = (&c_out, &u.csv) {
        write_bytes(p, &crate::textconv::encode(t, opts.encoding))?;
        scritti.push(p.clone());
        parti.push("CSV");
    }
    if let (Some(p), Some(b)) = (&a_out, &u.asm) {
        write_bytes(p, b)?;
        scritti.push(p.clone());
        parti.push("Allotrope");
    }
    if let (Some(p), Some(t)) = (&p_out, &u.piatto) {
        write_bytes(p, &crate::textconv::encode(t, opts.encoding))?;
        scritti.push(p.clone());
        parti.push("CSV piatto");
    }
    let Some(primo) = scritti.first().cloned() else { return Err(err("niente da scrivere")) };
    let n = l.conta();
    let strumento = [scelte::nome_strumento(&l), scelte::riga_lettura(&l)].join(" · ");
    ULTIMO.with(|x| {
        *x.borrow_mut() = Some(Esito { strumento, origine: (l.file.clone(), l.sha256.clone()), altri: scritti[1..].to_vec(), note: scelte::note(&l, &u, s) });
    });
    let message = format!("{} · {} {} → {}", l.nome, n, if n == 1 { "misura" } else { "misure" }, parti.join(" + "));
    Ok(Output { path: primo, message })
}
