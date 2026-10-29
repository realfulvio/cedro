//! Il lavoro di fondo della scheda PDF che non salva niente: quanto peserebbe un PDF dopo la
//! compressione (la stima nella coda di «Comprimi») e la pagina vera con numeri e filigrana
//! (l'anteprima di «Pagine»). Un thread solo, un lavoro alla volta; dell'anteprima conta solo
//! l'ultima richiesta, e le stime fatte con un livello che non è più quello scelto si lasciano cadere.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex};

use super::gfx::Image;
use super::model::Event;
use crate::pdfedit::{Squeeze, Stamp};

/// Lato lungo della pagina dell'anteprima con numeri e filigrana.
const STAMP_SIDE: f32 = 900.0;

enum Job {
    Estimate { path: PathBuf, squeeze: Squeeze, password: Option<String>, key: String },
    Stamp { path: PathBuf, pages: Vec<(u32, i32)>, stamp: Stamp, page: usize, password: Option<String>, key: String },
}

struct Queue {
    jobs: VecDeque<Job>,
    closed: bool,
}

pub struct Worker {
    queue: Arc<(Mutex<Queue>, Condvar)>,
}

/// La pagina `page` (da 0) del PDF come uscirebbe da «Pagine»: le pagine scelte, girate e timbrate.
pub fn stamped_page(path: &Path, pages: &[(u32, i32)], stamp: &Stamp, page: usize, password: Option<&str>, side: f32) -> Result<Image, String> {
    let bytes = std::fs::read(path).map_err(|e| e.to_string())?;
    let doc = crate::pdfpages::open(&bytes, password)?;
    let total = crate::pdfpages::page_count(&doc) as u32;
    let list: Vec<(u32, i32)> = if pages.is_empty() { (1..=total).map(|p| (p, 0)).collect() } else { pages.to_vec() };
    if list.iter().any(|(p, _)| *p == 0 || *p > total) || list.is_empty() {
        return Err("pagine non valide".into());
    }
    let settings = crate::pdfpages::Settings { file_bookmarks: false, duplex_blank: false, ..Default::default() };
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let built = crate::pdfpages::build(&[crate::pdfpages::Part { doc: &doc, pages: list.iter().map(|(p, _)| *p).collect(), title: stem }], &settings)?;
    let rotations: Vec<i32> = list.iter().map(|(_, r)| *r).collect();
    let out = crate::pdfedit::finish(&built, &rotations, stamp)?;
    let (w, h, rgba) = crate::ocr::render_page_rgba(&out, page.min(list.len() - 1), side).map_err(|e| e.to_string())?;
    Image::from_rgba(w, h, &rgba).ok_or_else(|| "pagina vuota".to_string())
}

impl Worker {
    pub fn new(tx: Sender<Event>, wake: Arc<dyn Fn() + Send + Sync>) -> Worker {
        let queue = Arc::new((Mutex::new(Queue { jobs: VecDeque::new(), closed: false }), Condvar::new()));
        let q = queue.clone();
        std::thread::spawn(move || loop {
            let job = {
                let (lock, cv) = &*q;
                let mut g = lock.lock().unwrap_or_else(|e| e.into_inner());
                while g.jobs.is_empty() && !g.closed {
                    g = cv.wait(g).unwrap_or_else(|e| e.into_inner());
                }
                if g.closed {
                    break;
                }
                g.jobs.pop_front()
            };
            let sent = match job {
                Some(Job::Estimate { path, squeeze, password, key }) => {
                    let result = std::panic::catch_unwind(|| crate::engine::squeeze_pdf(&path, &squeeze, password.as_deref()).map(|(out, before)| (before, out.len() as u64)))
                        .unwrap_or_else(|p| Err(super::model::panic_text(&*p)));
                    tx.send(Event::Estimate(path, key, result))
                }
                Some(Job::Stamp { path, pages, stamp, page, password, key }) => {
                    let img = std::panic::catch_unwind(|| stamped_page(&path, &pages, &stamp, page, password.as_deref(), STAMP_SIDE).ok()).unwrap_or_else(|p| {
                        super::model::panic_text(&*p);
                        None
                    });
                    tx.send(Event::StampPreview(key, img))
                }
                None => Ok(()),
            };
            if sent.is_err() {
                break;
            }
            wake();
        });
        Worker { queue }
    }

    /// Una stima in coda; quelle ancora da fare con un'altra chiave (un altro livello) si tolgono.
    pub fn estimate(&self, path: &Path, squeeze: Squeeze, password: Option<String>, key: &str) {
        let (lock, cv) = &*self.queue;
        let mut g = lock.lock().unwrap_or_else(|e| e.into_inner());
        g.jobs.retain(|j| !matches!(j, Job::Estimate { key: k, .. } if k != key));
        g.jobs.push_back(Job::Estimate { path: path.to_path_buf(), squeeze, password, key: key.to_string() });
        cv.notify_one();
    }

    /// L'anteprima con numeri e filigrana passa davanti alle stime e prende il posto di quella di prima.
    pub fn stamp(&self, path: &Path, pages: Vec<(u32, i32)>, stamp: Stamp, page: usize, password: Option<String>, key: &str) {
        let (lock, cv) = &*self.queue;
        let mut g = lock.lock().unwrap_or_else(|e| e.into_inner());
        g.jobs.retain(|j| !matches!(j, Job::Stamp { .. }));
        g.jobs.push_front(Job::Stamp { path: path.to_path_buf(), pages, stamp, page, password, key: key.to_string() });
        cv.notify_one();
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let (lock, cv) = &*self.queue;
        lock.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
        cv.notify_all();
    }
}
