//! Lo stato della sezione «Firma digitale» e del visualizzatore dei file firmati: i file da
//! verificare, gli ultimi aperti, il file mostrato nel visualizzatore e il suo documento.
//! Le verifiche girano in thread a parte: prima senza rete (subito), poi con la revoca online.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::firma::{self, At, Level, Opts, Report};

/// Un file della «Verifica di più file».
#[derive(Clone)]
pub struct Item {
    pub path: PathBuf,
    pub report: Option<Arc<Report>>,
    /// Si sta ancora controllando la revoca online.
    pub checking: bool,
    /// Venuto da una cartella: se risulta non firmato lo si toglie dall'elenco.
    pub from_folder: bool,
}

/// Un file aperto di recente.
#[derive(Clone, Debug, PartialEq)]
pub struct Recent {
    pub path: PathBuf,
    pub level: Level,
    pub headline: String,
    pub when: i64,
}

/// Il visualizzatore di un file firmato.
pub struct Viewer {
    pub path: PathBuf,
    pub report: Option<Arc<Report>>,
    pub checking: bool,
    /// Il documento estratto dalla busta, in una cartella di lavoro (per l'anteprima e «Apri con»).
    pub doc_path: Option<PathBuf>,
    /// La versione firmata di un PDF cambiato dopo (firma da cui viene), già scritta su disco.
    pub revision: Option<(usize, PathBuf)>,
    pub page: usize,
    /// 1.0 = la pagina sta tutta nella larghezza.
    pub zoom: f32,
    pub panel: bool,
    /// Le firme aperte nel pannello, e quelle con i dettagli tecnici aperti.
    pub open: HashSet<usize>,
    pub tech: HashSet<usize>,
    pub at: At,
    /// «Un altro giorno»: la data scritta (gg/mm/aaaa).
    pub at_text: String,
    pub at_error: bool,
}

type Inbox = Arc<Mutex<Vec<(PathBuf, u64, Report, bool)>>>;

pub struct FirmaUi {
    pub items: Vec<Item>,
    pub recent: Vec<Recent>,
    pub viewer: Option<Viewer>,
    inbox: Inbox,
    /// Ogni richiesta ha un numero: le risposte vecchie (data cambiata nel frattempo) si buttano.
    generation: u64,
    pub tl_busy: bool,
    tl_done: Arc<Mutex<Option<bool>>>,
    /// Le tre levette (apri i file firmati, anche i PDF, revoca online), lette all'apertura.
    pub opts: (bool, bool, bool),
    /// L'elenco dei certificatori: quando è stato scaricato e quanti sono.
    pub tl_info: (Option<i64>, usize),
    /// «Firma documenti», «Chiavetta e certificati» e «Opzioni della firma».
    pub sign: super::firmare::SignUi,
}

fn recent_file() -> PathBuf {
    crate::engine::config_dir().join("firma").join("recenti.txt")
}

fn level_code(l: Level) -> &'static str {
    match l {
        Level::Ok => "ok",
        Level::Warn => "warn",
        Level::Bad => "bad",
    }
}

pub fn load_recent() -> Vec<Recent> {
    let Ok(text) = std::fs::read_to_string(recent_file()) else { return Vec::new() };
    text.lines()
        .filter_map(|l| {
            let f: Vec<&str> = l.splitn(4, '\t').collect();
            if f.len() != 4 {
                return None;
            }
            let level = match f[1] {
                "ok" => Level::Ok,
                "warn" => Level::Warn,
                _ => Level::Bad,
            };
            Some(Recent { when: f[0].parse().ok()?, level, path: PathBuf::from(f[2]), headline: f[3].to_string() })
        })
        .collect()
}

fn save_recent(list: &[Recent]) {
    let p = recent_file();
    if let Some(d) = p.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let text: String = list
        .iter()
        .map(|r| format!("{}\t{}\t{}\t{}\n", r.when, level_code(r.level), r.path.display(), r.headline.replace(['\t', '\n'], " ")))
        .collect();
    let _ = std::fs::write(p, text);
}

/// Annota un file appena verificato tra gli ultimi aperti (anche da un altro processo, il visualizzatore).
pub fn remember(path: &Path, r: &Report) {
    if r.error.is_some() && r.signers.is_empty() {
        return;
    }
    let mut list = load_recent();
    list.retain(|x| x.path != path);
    list.insert(0, Recent { path: path.to_path_buf(), level: r.level, headline: r.headline.clone(), when: firma::now() });
    list.truncate(8);
    save_recent(&list);
}

/// La cartella dove si mettono i documenti estratti per mostrarli: una per processo, tolta all'uscita
/// (e quelle vecchie di più di un giorno al prossimo avvio).
fn work_dir() -> PathBuf {
    static DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
    DIR.get_or_init(|| {
        let base = std::env::temp_dir().join("cedro-firma");
        if let Ok(rd) = std::fs::read_dir(&base) {
            for e in rd.flatten() {
                let old = e.metadata().and_then(|m| m.modified()).ok().and_then(|t| t.elapsed().ok()).is_some_and(|d| d.as_secs() > 86400);
                if old {
                    let _ = std::fs::remove_dir_all(e.path());
                }
            }
        }
        base.join(format!("{}-{}", std::process::id(), firma::now()))
    })
    .clone()
}

static NEXT_DOC: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Scrive i byte in un file con quel nome nella cartella di lavoro.
pub fn write_temp(name: &str, data: &[u8]) -> Option<PathBuf> {
    let n = NEXT_DOC.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let dir = work_dir().join(n.to_string());
    std::fs::create_dir_all(&dir).ok()?;
    let clean: String = name.chars().map(|c| if c.is_control() || "\\/:*?\"<>|".contains(c) { '_' } else { c }).collect();
    let p = dir.join(if clean.trim().is_empty() { "documento".to_string() } else { clean });
    std::fs::write(&p, data).ok()?;
    Some(p)
}

impl Default for FirmaUi {
    fn default() -> Self {
        let s = crate::read_settings();
        FirmaUi {
            items: Vec::new(),
            recent: load_recent(),
            viewer: None,
            inbox: Arc::default(),
            generation: 0,
            tl_busy: false,
            tl_done: Arc::default(),
            opts: (s.firma_assoc, s.firma_pdf, s.firma_online),
            tl_info: firma::trust::summary(),
            sign: Default::default(),
        }
    }
}

impl Drop for FirmaUi {
    fn drop(&mut self) {
        if self.viewer.is_some() {
            let _ = std::fs::remove_dir_all(work_dir());
        }
        // Chiusa la finestra, l'elenco dei certificatori non resta in memoria.
        firma::trust::forget();
    }
}

impl FirmaUi {
    fn spawn(&mut self, path: PathBuf, at: At, detached: Option<Vec<u8>>, wake: Arc<dyn Fn() + Send + Sync>) -> u64 {
        self.generation += 1;
        let gen = self.generation;
        let inbox = self.inbox.clone();
        let online = crate::read_settings().firma_online;
        std::thread::spawn(move || {
            let opts = Opts { at, online: false, detached: detached.clone(), now: None };
            let first = std::panic::catch_unwind(|| firma::verify_path(&path, opts)).unwrap_or_else(|_| Report { error: Some("errore interno durante la verifica".into()), ..Default::default() });
            let need_online = online && !first.signers.is_empty();
            inbox.lock().unwrap_or_else(|e| e.into_inner()).push((path.clone(), gen, first, !need_online));
            wake();
            if need_online {
                let opts = Opts { at, online: true, detached, now: None };
                if let Ok(r) = std::panic::catch_unwind(|| firma::verify_path(&path, opts)) {
                    inbox.lock().unwrap_or_else(|e| e.into_inner()).push((path, gen, r, true));
                    wake();
                }
            }
        });
        gen
    }

    /// Aggiunge file alla verifica di più file.
    pub fn add(&mut self, paths: Vec<PathBuf>, wake: Arc<dyn Fn() + Send + Sync>) {
        firma::trust::refresh_if_old(|_| {});
        for p in paths {
            if p.is_dir() {
                let mut inner: Vec<PathBuf> = std::fs::read_dir(&p).map(|rd| rd.flatten().map(|e| e.path()).filter(|x| x.is_file()).collect()).unwrap_or_default();
                inner.sort();
                for q in inner {
                    if is_candidate(&q) && !self.items.iter().any(|i| i.path == q) {
                        self.items.push(Item { path: q.clone(), report: None, checking: true, from_folder: true });
                        self.spawn(q, At::Now, None, wake.clone());
                    }
                }
                continue;
            }
            if self.items.iter().any(|i| i.path == p) {
                continue;
            }
            self.items.push(Item { path: p.clone(), report: None, checking: true, from_folder: false });
            self.spawn(p, At::Now, None, wake.clone());
        }
    }

    /// Apre il visualizzatore su un file (in questa finestra).
    pub fn open_viewer(&mut self, path: PathBuf, wake: Arc<dyn Fn() + Send + Sync>) {
        firma::trust::refresh_if_old(|_| {});
        self.viewer = Some(Viewer {
            path: path.clone(),
            report: None,
            checking: true,
            doc_path: None,
            revision: None,
            page: 0,
            zoom: 1.0,
            panel: true,
            open: HashSet::new(),
            tech: HashSet::new(),
            at: At::Now,
            at_text: String::new(),
            at_error: false,
        });
        self.spawn(path, At::Now, None, wake);
    }

    /// Rifà la verifica del file nel visualizzatore (data cambiata, file originale scelto).
    pub fn reverify(&mut self, at: At, detached: Option<Vec<u8>>, wake: Arc<dyn Fn() + Send + Sync>) {
        let Some(v) = self.viewer.as_mut() else { return };
        v.at = at;
        v.checking = true;
        let path = v.path.clone();
        self.spawn(path, at, detached, wake);
    }

    /// Le risposte arrivate dai thread. Vero se è cambiato qualcosa.
    pub fn pump(&mut self) -> bool {
        let got: Vec<_> = std::mem::take(&mut *self.inbox.lock().unwrap_or_else(|e| e.into_inner()));
        let mut changed = self.sign.pump();
        if self.tl_done.lock().unwrap_or_else(|e| e.into_inner()).take().is_some() {
            self.tl_busy = false;
            self.tl_info = firma::trust::summary();
            changed = true;
        }
        for (path, gen, report, last) in got {
            changed = true;
            let report = Arc::new(report);
            if let Some(it) = self.items.iter_mut().find(|i| i.path == path) {
                it.report = Some(report.clone());
                it.checking = !last;
            }
            // In una cartella (allegati di una PEC) i file senza firma non interessano: spariscono.
            if report.unsigned() && self.items.iter().any(|i| i.path == path && i.from_folder) {
                self.items.retain(|i| i.path != path);
                if self.viewer.as_ref().is_none_or(|v| v.path != path) {
                    continue;
                }
            }
            if let Some(v) = self.viewer.as_mut().filter(|v| v.path == path) {
                if gen != self.generation {
                    continue;
                }
                if v.doc_path.is_none() && !report.doc.is_empty() {
                    v.doc_path = write_temp(&report.doc_name, &report.doc);
                }
                // Con una sola firma, o con una che non va, la si apre subito.
                if v.report.is_none() {
                    if report.signers.len() == 1 {
                        v.open.insert(0);
                    } else if let Some(i) = report.ordered().into_iter().find(|&i| report.signers[i].level == Level::Bad) {
                        v.open.insert(i);
                    }
                }
                v.report = Some(report.clone());
                v.checking = !last;
            }
            if last && !report.unsigned() {
                remember(&path, &report);
                self.recent = load_recent();
            }
        }
        changed
    }

    pub fn busy(&self) -> bool {
        self.items.iter().any(|i| i.checking) || self.viewer.as_ref().is_some_and(|v| v.checking) || self.tl_busy || self.sign.busy()
    }

    /// Riscarica subito l'elenco europeo dei certificatori.
    pub fn update_trust(&mut self, wake: Arc<dyn Fn() + Send + Sync>) {
        if self.tl_busy {
            return;
        }
        self.tl_busy = true;
        let done = self.tl_done.clone();
        std::thread::spawn(move || {
            let ok = match firma::trust::download() {
                Ok(st) => {
                    let _ = firma::trust::save(&st);
                    firma::trust::set_store(st);
                    true
                }
                Err(e) => {
                    crate::append_log(&format!("firma: elenco dei certificatori non scaricato: {e}"));
                    false
                }
            };
            *done.lock().unwrap_or_else(|e| e.into_inner()) = Some(ok);
            wake();
        });
    }

    /// I file da controllare per primi: quelli che non vanno, poi con avvisi, poi i buoni.
    pub fn sorted(&self) -> Vec<usize> {
        let mut v: Vec<usize> = (0..self.items.len()).collect();
        let rank = |i: &usize| match self.items[*i].report.as_ref().map(|r| r.level) {
            Some(Level::Bad) => 0,
            Some(Level::Warn) => 1,
            None => 2,
            Some(Level::Ok) => 3,
        };
        v.sort_by_key(rank);
        v
    }
}

/// Un file che può essere firmato (dal nome o perché è un PDF).
pub fn is_candidate(p: &Path) -> bool {
    let n = p.file_name().map(|n| n.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
    firma::is_signed_name(&n) || n.ends_with(".pdf") || n.ends_with(".xml")
}

/// «gg/mm/aaaa» → mezzogiorno di quel giorno in Italia (secondi UTC).
pub fn parse_day(s: &str) -> Option<i64> {
    let p: Vec<i64> = s.trim().split(['/', '-', '.']).filter_map(|x| x.trim().parse().ok()).collect();
    if p.len() != 3 || !(1..=31).contains(&p[0]) || !(1..=12).contains(&p[1]) || !(1900..=2200).contains(&p[2]) {
        return None;
    }
    Some(firma::asn1::days_from_civil(p[2], p[1], p[0]) * 86400 + 11 * 3600)
}

/// Apre il visualizzatore in una finestra a parte (un altro processo, come col doppio clic).
pub fn launch_viewer(path: &Path) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(exe).arg("verifica").arg(path).spawn().map(|_| ()).map_err(|e| format!("non riesco ad aprire il visualizzatore: {e}"))
}

/// Salva il documento estratto accanto al file firmato (senza sostituire nulla). Ritorna dove.
pub fn save_beside(signed: &Path, name: &str, data: &[u8]) -> Result<PathBuf, String> {
    let dir = signed.parent().map(Path::to_path_buf).unwrap_or_default();
    let mut target = dir.join(name);
    if target == signed || target.exists() {
        let stem = Path::new(name).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| name.into());
        let ext = Path::new(name).extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
        let mut k = 1;
        loop {
            target = dir.join(format!("{stem} ({k}){ext}"));
            if !target.exists() {
                break;
            }
            k += 1;
        }
    }
    std::fs::write(&target, data).map_err(|e| format!("non riesco a salvare: {e}"))?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    #[test]
    fn data() {
        let t = super::parse_day("03/10/2026").unwrap();
        assert_eq!(crate::firma::date(t), "03/10/2026");
        assert!(super::parse_day("31/13/2026").is_none());
    }
}
