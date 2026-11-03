//! «Riassunto AI» in Rivedi: il lavoro in corso, cosa arriva dal motore e cosa resta a lavoro finito.
//! Il riassunto vero lo fa `crate::riassunto`; il disegno sta in `views/revisione.rs`.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

use super::model::{App, Event, Notice};
use crate::riassunto::{self, Failure, Progress, Step};

/// Il riassunto che si sta facendo. Resta anche se si chiude Rivedi: riaprendo lo si ritrova.
pub struct Job {
    pub source: PathBuf,
    pub cancel: Arc<AtomicBool>,
    pub step: Step,
    pub percent: u8,
    /// La RAM libera e quella del PC quando è partito, per i passi e la barra della memoria.
    pub free: Option<u64>,
    pub total: Option<u64>,
    /// Da quando legge la riunione (per dire quanto manca) e su cosa lavora.
    pub reading_since: Option<Instant>,
    pub gpu: Option<String>,
    /// Vero quando il modello è carico e il motore è pronto.
    pub ready: bool,
}

impl Job {
    /// «Circa 2 minuti ancora»: dal tempo che ci ha messo finora, appena ce n'è abbastanza per stimarlo.
    pub fn left_text(&self) -> Option<String> {
        let since = self.reading_since?;
        if self.percent < 5 {
            return None;
        }
        let spent = since.elapsed().as_secs_f64();
        let left = spent * (100.0 - self.percent as f64) / self.percent as f64;
        Some(match left {
            l if l < 45.0 => "Meno di 1 minuto ancora".to_string(),
            l if l < 90.0 => "Circa 1 minuto ancora".to_string(),
            l => format!("Circa {} minuti ancora", (l / 60.0).round() as u64),
        })
    }
}

impl App {
    /// Il riassunto in corso per questo file, se c'è.
    pub fn summary_job_for(&self, source: &Path) -> Option<&Job> {
        self.summary_job.as_ref().filter(|j| j.source == source)
    }

    /// Com'è finito l'ultimo riassunto non riuscito di questo file.
    pub fn summary_failure_for(&self, source: &Path) -> Option<&Failure> {
        self.summary_failure.as_ref().filter(|(p, _)| p == source).map(|(_, f)| f)
    }

    /// «Fai il riassunto», «Riprova», «Rifai»: parte con le frasi e i nomi di adesso.
    pub fn summary_start(&mut self) {
        let Some(r) = self.review.as_ref() else { return };
        if let Some(job) = &self.summary_job {
            if job.source != r.source {
                let name = job.source.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
                self.notice = Some(Notice { text: format!("Sta già facendo il riassunto di {}: aspetta che finisca.", crate::i18n::lit(&name)), error: false });
            }
            return;
        }
        let names = r.current_names();
        let lines = riassunto::lines_for(&r.segments(), if r.names.is_empty() { &[] } else { &names });
        let source = r.source.clone();
        let settings = crate::read_settings();
        let opts = riassunto::Options { gpu: settings.gpu, avx512: settings.avx512 };
        let cancel = Arc::new(AtomicBool::new(false));
        self.summary_failure = None;
        self.summary_job = Some(Job {
            source: source.clone(),
            cancel: cancel.clone(),
            step: Step::Loading,
            percent: 0,
            free: crate::resources::available_memory(),
            total: riassunto::total_memory(),
            reading_since: None,
            gpu: None,
            ready: false,
        });
        let (tx, wake) = (self.tx.clone(), self.wake.clone());
        std::thread::spawn(move || {
            let send = |p: Progress| {
                let _ = tx.send(Event::Summary(source.clone(), p));
                wake();
            };
            let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| riassunto::run(lines, names, opts, &cancel, &send)));
            if let Err(p) = run {
                send(Progress::Done(Err(Failure::Other(super::model::panic_text(&*p)))));
            }
        });
    }

    pub fn summary_cancel(&mut self) {
        if let Some(job) = &self.summary_job {
            job.cancel.store(true, Ordering::Relaxed);
        }
    }

    /// Quello che arriva dal motore.
    pub(super) fn summary_event(&mut self, source: &Path, p: Progress) {
        let Some(job) = self.summary_job.as_mut().filter(|j| j.source == source) else { return };
        match p {
            Progress::Step(step) => {
                if matches!(step, Step::Reading(..)) && job.reading_since.is_none() {
                    job.reading_since = Some(Instant::now());
                }
                job.step = step;
            }
            Progress::Ready { gpu } => {
                job.ready = true;
                job.gpu = gpu;
            }
            Progress::Percent(p) => job.percent = job.percent.max(p),
            Progress::Done(result) => {
                self.summary_job = None;
                crate::accel::clear_in_use();
                match result {
                    Ok(stored) => {
                        // Resta con la revisione: riaprendo Rivedi il riassunto c'è ancora.
                        let _ = crate::revisione::store_summary(source, &stored);
                        if let Some(r) = self.review.as_mut().filter(|r| r.source == source) {
                            r.summary = Some(stored);
                            r.summary_on_top = true;
                            r.summary_copied = false;
                        }
                    }
                    Err(Failure::Cancelled) => {}
                    Err(f) => self.summary_failure = Some((source.to_path_buf(), f)),
                }
            }
        }
    }

    /// «Riassunto AI» nella coda: apre Rivedi sulla scheda del riassunto e, se non c'è ancora, lo fa subito.
    pub fn summary_open(&mut self, index: usize) {
        self.review_open(index);
        let Some(r) = self.review.as_mut() else { return };
        r.tab = 2;
        let source = r.source.clone();
        if r.summary.is_none() && self.summary_job_for(&source).is_none() {
            self.summary_start();
        }
    }

    /// Spunta (o toglie la spunta a) una cosa da fare: resta salvata col riassunto.
    pub fn summary_toggle_todo(&mut self, k: usize) {
        let Some(r) = self.review.as_mut() else { return };
        let Some(s) = r.summary.as_mut() else { return };
        match s.done.iter().position(|d| *d == k) {
            Some(i) => {
                s.done.remove(i);
            }
            None => s.done.push(k),
        }
        let _ = crate::revisione::store_summary(&r.source, s);
    }
}
