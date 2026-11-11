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
    /// Pagine dei PDF e dell'anteprima di testi e documenti.
    pub pages: Option<usize>,
    pub thumb: Option<Image>,
    /// I vetrini digitali (.svs): livelli e riga di descrizione.
    pub slide: Option<SlideInfo>,
    /// I DICOM: la riga che li descrive e se il nome può essere scritto nei pixel.
    pub dicom: Option<DicomInfo>,
    /// La scheda di audio, video e altri file.
    pub about: Option<super::preview::About>,
}

/// Quello che il pannello mostra di un vetrino: la riga della coda e le misure dei livelli.
#[derive(Clone, Debug, Default)]
pub struct SlideInfo {
    /// La riga che lo descrive, in italiano e in inglese: «Aperio · 3 livelli · tessere JPEG · 20× (0,499 µm/px)».
    pub summary: [String; 2],
    /// Larghezza e altezza del livello di ogni scelta: Panoramica, Media, Piena (e quale livello è).
    pub picks: [(usize, u32, u32); 3],
    /// Le divisioni rispetto al pieno di ogni scelta (1 per il pieno).
    pub ratios: [u32; 3],
    pub label: bool,
    pub macro_image: bool,
}

impl SlideInfo {
    pub fn of(s: &crate::svs::Slide) -> SlideInfo {
        use crate::svs::Pick;
        let full = s.full().width.max(1);
        let one = |p: Pick| {
            let i = s.pick(p);
            let l = &s.levels[i];
            ((i, l.width, l.height), (full as f32 / l.width.max(1) as f32).round().max(1.0) as u32)
        };
        let (a, b, c) = (one(Pick::Overview), one(Pick::Medium), one(Pick::Full));
        SlideInfo { summary: [s.summary(false), s.summary(true)], picks: [a.0, b.0, c.0], ratios: [a.1, b.1, c.1], label: s.label.is_some(), macro_image: s.macro_image.is_some() }
    }
}

/// Quello che il pannello mostra di un DICOM.
#[derive(Clone, Debug, Default)]
pub struct DicomInfo {
    /// «TC · 120 immagini · 512 × 512 · 0,5 mm», in italiano e in inglese.
    pub summary: [String; 2],
    pub modality: String,
    pub frames: u32,
    /// Ecografie e catture dello schermo: il nome può essere dentro l'immagine.
    pub burned_in: bool,
}

impl DicomInfo {
    pub fn of(f: &crate::dicom::DicomFile) -> DicomInfo {
        let info = crate::dicom::pixel::Info::of(f).ok();
        let modality = f.string(crate::dicom::tag(0x0008, 0x0060)).unwrap_or_default();
        let frames = info.as_ref().map(|i| i.frames).unwrap_or(0);
        let line = |english: bool| {
            let mut parts: Vec<String> = Vec::new();
            let name = crate::dicom::modality_name(&modality);
            parts.push(if name.is_empty() { "DICOM".into() } else if english { modality.clone() } else { name.to_string() });
            match &info {
                Some(i) => {
                    if i.frames > 1 {
                        parts.push(if english { format!("{} images", i.frames) } else { format!("{} immagini", i.frames) });
                    }
                    parts.push(format!("{} × {}", i.cols, i.rows));
                    if let Some((r, _)) = i.spacing {
                        let mm = format!("{r:.2}").trim_end_matches('0').trim_end_matches('.').to_string();
                        parts.push(format!("{} mm", if english { mm } else { mm.replace('.', ",") }));
                    }
                }
                None => parts.push(if english { "no image (report or list)".into() } else { "senza immagine (referto o elenco)".into() }),
            }
            parts.join(" · ")
        };
        DicomInfo { summary: [line(false), line(true)], modality, frames, burned_in: crate::dicom::pixel::burned_in(f) }
    }
}

enum Job {
    Info(PathBuf),
    Preview(PathBuf, usize),
    /// Le miniature di alcune pagine di un PDF, lette con una sola apertura del file.
    Thumbs(PathBuf, Vec<usize>),
}

struct Queue {
    jobs: VecDeque<Job>,
    /// La finestra è chiusa: il thread finisce invece di aspettare per sempre.
    closed: bool,
}

pub struct Loader {
    queue: Arc<(Mutex<Queue>, Condvar)>,
}

/// Lato lungo delle miniature (in pixel): basta per la coda e per la scheda grande.
const THUMB: u32 = 360;
/// Lato lungo delle pagine dell'anteprima.
const PAGE: f32 = 1400.0;
/// Lato lungo delle miniature delle pagine (striscia di «Dividi», pagine del PDF unito).
const PAGE_THUMB: f32 = 240.0;

/// Le miniature di più pagine di un PDF: il file si legge e si analizza una volta sola.
pub fn pdf_page_thumbs(path: &Path, pages: &[usize], side: f32, each: &mut dyn FnMut(usize, Option<Image>) -> bool) {
    use hayro::hayro_interpret::InterpreterSettings;
    use hayro::hayro_syntax::Pdf;
    use hayro::vello_cpu::color::palette::css::WHITE;
    use hayro::{render, PixmapSettings, RenderCache, RenderSettings};
    let pdf = std::fs::read(path).ok().and_then(|b| Pdf::new(b).ok());
    let Some(pdf) = pdf else {
        for &p in pages {
            if !each(p, None) {
                return;
            }
        }
        return;
    };
    let all = pdf.pages();
    let cache = RenderCache::new();
    for &n in pages {
        let img = all.get(n).and_then(|p| {
            let (w, h) = p.render_dimensions();
            let scale = side / w.max(h).max(1.0);
            let pixmap = PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE };
            let r = render(p, &cache, &InterpreterSettings::default(), &RenderSettings::default(), &pixmap);
            let (w, h) = (r.width() as u32, r.height() as u32);
            (w > 0 && h > 0).then(|| Image::from_rgba(w, h, &r.take_rgba8(hayro::vello_cpu::peniko::ImageAlphaType::Alpha))).flatten()
        });
        if !each(n, img) {
            return;
        }
    }
}

pub fn image_thumb(path: &Path, side: u32) -> Option<(Image, (u32, u32))> {
    use image::ImageDecoder;
    // Un vetrino: la sua miniatura (o il livello più piccolo), mai il livello pieno; le misure sono quelle del pieno.
    if crate::svs::is_slide(path) {
        let s = crate::svs::Slide::open(path).ok()?;
        let small = s.preview(path, side).ok()?;
        let rgba = image::DynamicImage::ImageRgb8(small).to_rgba8();
        return Some((Image::from_rgba(rgba.width(), rgba.height(), rgba.as_raw())?, (s.full().width, s.full().height)));
    }
    // SVG, AVIF, JPEG XL e DICOM non passano dal lettore del crate `image`.
    if matches!(crate::imgfmt::sniff_file(path), Some("svg" | "avif" | "jxl" | "dcm")) || matches!(engine::extension(path).as_str(), "svg" | "svgz" | "dcm" | "dicom") {
        let img = crate::imgfmt::open(path, false, Some(side)).ok()?;
        let size = (img.width(), img.height());
        let small = img.thumbnail(side, side).to_rgba8();
        return Some((Image::from_rgba(small.width(), small.height(), small.as_raw())?, size));
    }
    let mut decoder = image::ImageReader::open(path).ok()?.with_guessed_format().ok()?.into_decoder().ok()?;
    let orientation = decoder.orientation().ok();
    let mut img = image::DynamicImage::from_decoder(decoder).ok()?;
    if let Some(o) = orientation {
        img.apply_orientation(o);
    }
    let size = (img.width(), img.height());
    let small = img.thumbnail(side, side).to_rgba8();
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
            if let Some((thumb, size)) = image_thumb(path, THUMB) {
                out.thumb = Some(thumb);
                out.size = Some(size);
            }
            if crate::svs::is_slide(path) {
                out.slide = crate::svs::Slide::open(path).ok().map(|s| SlideInfo::of(&s));
            }
            if crate::imgfmt::sniff_file(path) == Some("dcm") || matches!(engine::extension(path).as_str(), "dcm" | "dicom") {
                out.dicom = crate::dicom::DicomFile::open(path).ok().map(|f| DicomInfo::of(&f));
            }
        }
        Kind::Pdf => {
            if let Ok(bytes) = std::fs::read(path) {
                out.pages = crate::ocr::pdf_page_count(&bytes).ok();
                out.thumb = crate::ocr::render_page_rgba(&bytes, 0, THUMB as f32).ok().and_then(|(w, h, rgba)| Image::from_rgba(w, h, &rgba));
            }
        }
        Kind::Text | Kind::Doc => out.pages = super::preview::page_count(path),
        Kind::Audio => out.about = Some(super::preview::about_audio(path, out.bytes)),
        Kind::Video => {
            let about = super::preview::about_video(path, out.bytes);
            out.thumb = about.frames.first().cloned();
            out.about = Some(about);
        }
        Kind::Other | Kind::Archive => out.about = Some(super::preview::about_other(path)),
    }
    out
}

impl Loader {
    pub fn new(tx: Sender<Event>, wake: Arc<dyn Fn() + Send + Sync>) -> Loader {
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
            // Se la finestra non c'è più, il canale è chiuso e il thread finisce.
            let sent = match job {
                // Un file che manda in crisi un decodificatore resta senza anteprima, ma il thread va avanti con gli altri.
                Some(Job::Info(path)) => {
                    let loaded = std::panic::catch_unwind(|| info(&path)).unwrap_or_else(|p| {
                        super::model::panic_text(&*p);
                        Loaded::default()
                    });
                    tx.send(Event::Media(path, loaded))
                }
                Some(Job::Preview(path, page)) => {
                    let img = std::panic::catch_unwind(|| match engine::kind_of(&path) {
                        Kind::Pdf => pdf_page(&path, page, PAGE),
                        Kind::Image => image_thumb(&path, PAGE as u32).map(|(i, _)| i),
                        Kind::Text | Kind::Doc => super::preview::render_page(&path, page, PAGE),
                        _ => None,
                    })
                    .unwrap_or_else(|p| {
                        super::model::panic_text(&*p);
                        None
                    });
                    tx.send(Event::Preview(path, page, img))
                }
                Some(Job::Thumbs(path, pages)) => {
                    let mut ok = Ok(());
                    let run = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                        pdf_page_thumbs(&path, &pages, PAGE_THUMB, &mut |page, img| {
                            ok = tx.send(Event::PageThumb(path.clone(), page, img));
                            wake();
                            ok.is_ok()
                        })
                    }));
                    if let Err(p) = run {
                        super::model::panic_text(&*p);
                    }
                    ok
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

    /// Le miniature delle pagine vengono dopo le informazioni dei file già chieste.
    pub fn page_thumbs(&self, path: &Path, pages: Vec<usize>) {
        let (lock, cv) = &*self.queue;
        lock.lock().unwrap_or_else(|e| e.into_inner()).jobs.push_back(Job::Thumbs(path.to_path_buf(), pages));
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

impl Drop for Loader {
    fn drop(&mut self) {
        let (lock, cv) = &*self.queue;
        lock.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
        cv.notify_all();
    }
}

/// La miniatura di un file (immagine o prima pagina di un PDF) per chi la vuole subito, fuori dal thread dei file.
pub fn thumb_of(path: &Path, side: u32) -> Option<Image> {
    match engine::kind_of(path) {
        Kind::Image => image_thumb(path, side).map(|(i, _)| i),
        Kind::Pdf => pdf_page(path, 0, side as f32),
        Kind::Text | Kind::Doc => super::preview::render_page(path, 0, side as f32),
        _ => None,
    }
}
