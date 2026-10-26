//! Cosa si sa dei file in coda senza aprirli nel thread della finestra: peso, misure, numero di
//! pagine, la miniatura e le pagine dell'anteprima dei documenti. Un thread a parte le prepara
//! una alla volta; le pagine dell'anteprima hanno la precedenza sulle miniature.

use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex};

use super::gfx::Image;
use super::model::Event;
use crate::engine::{self, Kind};

/// Quello che si è letto di un file.
#[derive(Clone, Default)]
pub struct Loaded {
    pub bytes: u64,
    /// Larghezza e altezza delle immagini.
    pub size: Option<(u32, u32)>,
    /// Pagine dei PDF.
    pub pages: Option<usize>,
    pub thumb: Option<Image>,
}

enum Job {
    Info(PathBuf),
    Preview(PathBuf, usize),
}

struct Queue {
    jobs: VecDeque<Job>,
}

pub struct Loader {
    queue: Arc<(Mutex<Queue>, Condvar)>,
}

/// Lato lungo delle miniature (in pixel): basta per la coda e per la scheda grande.
const THUMB: u32 = 360;
/// Lato lungo delle pagine dell'anteprima.
const PAGE: f32 = 1400.0;

fn image_thumb(path: &Path) -> Option<(Image, (u32, u32))> {
    use image::ImageDecoder;
    let mut decoder = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.into_decoder().ok()?;
    let orientation = decoder.orientation().ok();
    let mut img = image::DynamicImage::from_decoder(decoder).ok()?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    let size = (img.width(), img.height());
    let small = img.thumbnail(THUMB, THUMB).to_rgba8();
    Some((Image::from_rgba(small.width(), small.height(), small.as_raw())?, size))
}

fn pdf_page(path: &Path, page: usize, side: f32) -> Option<Image> {
    let bytes = std::fs::read(path).ok()?;
    let (w, h, rgba) = crate::ocr::render_page_rgba(&bytes, page, side).ok()?;
    Image::from_rgba(w, h, &rgba)
}

fn info(path: &Path) -> Loaded {
    let mut out = Loaded { bytes: std::fs::metadata(path).map(|m| m.len()).unwrap_or(0), ..Default::default() };
    match engine::kind_of(path) {
        Kind::Image => {
            if let Some((thumb, size)) = image_thumb(path) {
                out.thumb = Some(thumb);
                out.size = Some(size);
            }
        }
        Kind::Pdf => {
            if let Ok(bytes) = std::fs::read(path) {
                out.pages = crate::ocr::pdf_page_count(&bytes).ok();
                out.thumb = crate::ocr::render_page_rgba(&bytes, 0, THUMB as f32).ok().and_then(|(w, h, rgba)| Image::from_rgba(w, h, &rgba));
            }
        }
        _ => {}
    }
    out
}

impl Loader {
    pub fn new(tx: Sender<Event>, wake: Arc<dyn Fn() + Send + Sync>) -> Loader {
        let queue = Arc::new((Mutex::new(Queue { jobs: VecDeque::new() }), Condvar::new()));
        let q = queue.clone();
        std::thread::spawn(move || loop {
            let job = {
                let (lock, cv) = &*q;
                let mut g = lock.lock().unwrap_or_else(|e| e.into_inner());
                while g.jobs.is_empty() {
                    g = cv.wait(g).unwrap_or_else(|e| e.into_inner());
                }
                g.jobs.pop_front()
            };
            // Se la finestra non c'è più, il canale è chiuso e il thread finisce.
            let sent = match job {
                Some(Job::Info(path)) => {
                    let loaded = info(&path);
                    tx.send(Event::Media(path, loaded))
                }
                Some(Job::Preview(path, page)) => {
                    let img = match engine::kind_of(&path) {
                        Kind::Pdf => pdf_page(&path, page, PAGE),
                        Kind::Image => image_thumb(&path).map(|(i, _)| i),
                        _ => None,
                    };
                    tx.send(Event::Preview(path, page, img))
                }
                None => Ok(()),
            };
            if sent.is_err() {
                break;
            }
            wake();
        });
        Loader { queue }
    }

    pub fn request(&self, path: &Path) {
        let (lock, cv) = &*self.queue;
        lock.lock().unwrap_or_else(|e| e.into_inner()).jobs.push_back(Job::Info(path.to_path_buf()));
        cv.notify_one();
    }

    /// Le pagine dell'anteprima passano davanti alle miniature: sono quelle che si stanno guardando.
    pub fn preview(&self, path: &Path, page: usize) {
        let (lock, cv) = &*self.queue;
        let mut g = lock.lock().unwrap_or_else(|e| e.into_inner());
        g.jobs.retain(|j| !matches!(j, Job::Preview(..)));
        g.jobs.push_front(Job::Preview(path.to_path_buf(), page));
        cv.notify_one();
    }
}
