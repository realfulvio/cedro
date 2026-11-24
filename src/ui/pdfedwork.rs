//! Il lavoro di fondo dell'editor dei PDF: apre il file, applica le modifiche in una copia e ne disegna le
//! pagine, cerca e sostituisce il testo, salva. Un thread solo, un lavoro alla volta: i disegni vecchi
//! (di una versione che non è più quella di adesso) si lasciano cadere prima di cominciarli.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Condvar, Mutex};

use super::gfx::Image;
use super::model::Event;
use crate::engine::Exists;
use crate::pdfmod::text::Why;
use crate::pdfmod::{self, find, Align, Edit, Opened, Rect, SaveOpts, Word};

/// Le modifiche com'erano a una certa versione: il numero cambia a ogni modifica, e con lui i disegni.
#[derive(Clone)]
pub struct Snap {
    pub rev: u64,
    pub edits: Arc<Vec<Edit>>,
}

/// Un paragrafo della pagina, quanto basta alla schermata per sceglierlo e dire come è scritto.
#[derive(Clone, Debug)]
pub struct Blk {
    pub rect: Rect,
    pub text: String,
    pub size: f32,
    pub color: [u8; 3],
    pub font_name: String,
    pub bold: bool,
    pub italic: bool,
    pub align: Align,
    pub why: Why,
    /// Le lettere che il carattere del file sa scrivere.
    pub has: Arc<HashSet<char>>,
}

impl Blk {
    pub fn editable(&self) -> bool {
        self.why == Why::Ok
    }
}

/// Il testo di una pagina come serve alla schermata.
#[derive(Clone, Debug, Default)]
pub struct PageText {
    pub w: f32,
    pub h: f32,
    pub blocks: Vec<Blk>,
    pub words: Vec<Word>,
    /// Una scansione con lo strato di testo dell'OCR: si cerca e si evidenzia, ma non si riscrive.
    pub scan: bool,
}

/// Un file aperto.
pub struct Loaded {
    pub path: PathBuf,
    pub opened: Opened,
}

pub enum Msg {
    Opened(Result<Box<Loaded>, String>),
    Page { rev: u64, page: usize, side: u32, img: Option<Image> },
    Thumb { rev: u64, page: usize, img: Option<Image> },
    Text { rev: u64, page: usize, text: Option<Arc<PageText>> },
    Failed(u64, String),
    Found { id: u64, hits: Result<Vec<(usize, find::Hit)>, String> },
    Patterns { id: u64, hits: Result<Vec<(usize, find::Pattern, find::Hit)>, String> },
    Replaced { id: u64, edits: Result<(Vec<Edit>, usize), String> },
    Saved(Result<(PathBuf, String), String>),
    /// Quanti caratteri, immagini e commenti toglierebbe l'oscuramento applicato (per dirlo a chi lo ha fatto).
    Reported { rev: u64, glyphs: usize, images: usize, annots: usize },
}

pub enum Job {
    Open(PathBuf),
    /// Si è chiuso l'editor: la copia in memoria non serve più.
    Release,
    Report { snap: Snap },
    Render { snap: Snap, pages: Vec<(usize, u32)>, thumbs: Vec<usize>, text: Vec<usize> },
    Find { snap: Snap, id: u64, query: String, case: bool, whole: bool },
    Patterns { snap: Snap, id: u64, which: Vec<find::Pattern> },
    Replace { snap: Snap, id: u64, query: String, with: String, case: bool, whole: bool },
    Save { snap: Snap, opts: SaveOpts, source: PathBuf, dir: Option<PathBuf>, exists: Exists },
}

struct Queue {
    jobs: VecDeque<Job>,
    closed: bool,
}

pub struct Worker {
    queue: Arc<(Mutex<Queue>, Condvar)>,
}

/// Lo stato del thread: il file com'era e l'ultima copia modificata (un solo conto per tutte le pagine).
struct State {
    orig: Option<Arc<Vec<u8>>>,
    cache: Option<(u64, Arc<Vec<u8>>)>,
}

impl State {
    fn derived(&mut self, snap: &Snap) -> Result<Arc<Vec<u8>>, String> {
        let orig = self.orig.clone().ok_or("nessun file aperto")?;
        if let Some((rev, bytes)) = &self.cache {
            if *rev == snap.rev {
                return Ok(bytes.clone());
            }
        }
        let bytes = if snap.edits.is_empty() { orig } else { Arc::new(pdfmod::apply(&orig, &snap.edits, &SaveOpts::default())?) };
        self.cache = Some((snap.rev, bytes.clone()));
        Ok(bytes)
    }
}

/// Dal testo letto dal motore a quello che serve alla schermata.
pub fn page_text(tp: &pdfmod::TextPage) -> PageText {
    let mut sets: HashMap<usize, HashSet<char>> = HashMap::new();
    for g in &tp.glyphs {
        sets.entry(g.font).or_default().extend(g.ch.chars());
    }
    let sets: HashMap<usize, Arc<HashSet<char>>> = sets.into_iter().map(|(k, v)| (k, Arc::new(v))).collect();
    let empty = Arc::new(HashSet::new());
    let blocks: Vec<Blk> = tp
        .blocks
        .iter()
        .map(|b| Blk {
            rect: b.rect,
            text: b.text.clone(),
            size: b.size,
            color: b.color,
            font_name: b.font_name.clone(),
            bold: b.bold,
            italic: b.italic,
            align: b.align,
            why: if tp.complete { b.why } else if b.why == Why::Ok { Why::Font } else { b.why },
            has: sets.get(&b.font).cloned().unwrap_or_else(|| empty.clone()),
        })
        .collect();
    let scan = blocks.iter().any(|b| b.why == Why::Scan);
    PageText { w: tp.w, h: tp.h, blocks, words: tp.words.clone(), scan }
}

/// Disegna le pagine `pages` (da 0) di una copia del PDF: (pagina, lato lungo in pixel) → immagine.
fn render_pages(bytes: &[u8], pages: &[(usize, u32)]) -> Vec<(usize, u32, Option<Image>)> {
    use hayro::hayro_interpret::InterpreterSettings;
    use hayro::hayro_syntax::Pdf;
    use hayro::vello_cpu::color::palette::css::WHITE;
    use hayro::{render, PixmapSettings, RenderCache, RenderSettings};

    let Ok(pdf) = Pdf::new(bytes.to_vec()) else { return pages.iter().map(|(p, s)| (*p, *s, None)).collect() };
    let all = pdf.pages();
    let settings = InterpreterSettings::default();
    pages
        .iter()
        .map(|&(page, side)| {
            let img = all.get(page).and_then(|p| {
                let (w, h) = p.render_dimensions();
                let scale = side as f32 / w.max(h).max(1.0);
                let pixmap = PixmapSettings { x_scale: scale, y_scale: scale, bg_color: WHITE };
                let rendered = render(p, &RenderCache::new(), &settings, &RenderSettings::default(), &pixmap);
                let (w, h) = (rendered.width() as u32, rendered.height() as u32);
                if w == 0 || h == 0 {
                    return None;
                }
                let rgba = rendered.take_rgba8(hayro::vello_cpu::peniko::ImageAlphaType::Alpha);
                Image::from_rgba(w, h, &rgba)
            });
            (page, side, img)
        })
        .collect()
}

fn is_boundary(c: &[char], a: usize, b: usize) -> bool {
    (a == 0 || !c[a - 1].is_alphanumeric()) && (b >= c.len() || !c[b].is_alphanumeric())
}

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

/// Il testo con ogni `query` sostituita da `with`: (testo nuovo, quante volte).
pub fn replace_in(text: &str, query: &str, with: &str, case: bool, whole: bool) -> (String, usize) {
    let q: Vec<char> = query.split_whitespace().collect::<Vec<_>>().join(" ").chars().collect();
    if q.is_empty() {
        return (text.to_string(), 0);
    }
    let chars: Vec<char> = text.chars().collect();
    let norm = |c: char| if case { c } else { fold(c) };
    let hay: Vec<char> = chars.iter().map(|c| norm(*c)).collect();
    let needle: Vec<char> = q.iter().map(|c| norm(*c)).collect();
    let (mut out, mut n, mut i) = (String::new(), 0, 0);
    while i < chars.len() {
        if i + needle.len() <= hay.len() && hay[i..i + needle.len()] == needle[..] && (!whole || is_boundary(&chars, i, i + needle.len())) {
            out.push_str(with);
            n += 1;
            i += needle.len();
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    (out, n)
}

/// Le modifiche che sostituiscono il testo cercato in ogni paragrafo che lo contiene.
fn replace_edits(bytes: &[u8], query: &str, with: &str, case: bool, whole: bool) -> Result<(Vec<Edit>, usize), String> {
    let mut edits = Vec::new();
    let mut count = 0;
    for (page, tp) in pdfmod::read_all_texts(bytes)?.into_iter().enumerate() {
        for b in tp.blocks.iter().filter(|b| b.editable() && tp.complete) {
            let (text, n) = replace_in(&b.text, query, with, case, whole);
            if n > 0 {
                count += n;
                edits.push(Edit::EditText { page, rect: b.rect, text, size: None, bold: None, italic: None, color: None, font: pdfmod::FontChoice::Same, align: None });
            }
        }
    }
    Ok((edits, count))
}

/// Dove va il file salvato: accanto all'originale (o nella cartella scelta), con «-modificato» in fondo.
/// `None` = non si salva (il file c'è già e la scelta è saltarlo, o si è risposto di saltarlo).
fn target_for(source: &Path, dir: Option<&Path>, exists: Exists, tx: &Sender<Event>) -> Result<PathBuf, String> {
    let stem = source.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| "documento".into());
    let dir = dir.map(Path::to_path_buf).or_else(|| source.parent().map(Path::to_path_buf)).unwrap_or_default();
    let name = |n: u32| if n <= 1 { format!("{stem}-modificato.pdf") } else { format!("{stem}-modificato-{n}.pdf") };
    let same = |p: &Path| std::fs::canonicalize(p).ok().zip(std::fs::canonicalize(source).ok()).is_some_and(|(a, b)| a == b);
    let numbered = || (2..10_000).map(|n| dir.join(name(n))).find(|p| !p.exists()).ok_or_else(|| "troppi file con lo stesso nome".to_string());
    let first = dir.join(name(1));
    if !first.exists() {
        return Ok(first);
    }
    let choice = match exists {
        Exists::Number => crate::engine::ExistsChoice::KeepBoth,
        Exists::Replace => crate::engine::ExistsChoice::Replace,
        Exists::Skip => crate::engine::ExistsChoice::Skip,
        Exists::Ask => {
            let (rtx, rrx) = std::sync::mpsc::channel();
            tx.send(Event::Ask(first.clone(), rtx)).map_err(|_| "finestra chiusa".to_string())?;
            rrx.recv().unwrap_or(crate::engine::ExistsChoice::Skip)
        }
    };
    match choice {
        crate::engine::ExistsChoice::Replace if !same(&first) => Ok(first),
        crate::engine::ExistsChoice::Replace | crate::engine::ExistsChoice::KeepBoth => numbered(),
        crate::engine::ExistsChoice::Skip => Err(format!("{} esiste già: non l'ho salvato", first.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default())),
    }
}

fn write_atomic(target: &Path, bytes: &[u8]) -> Result<(), String> {
    let tmp = target.with_extension("pdf.cedro-tmp");
    std::fs::write(&tmp, bytes).map_err(|e| format!("non riesco a scrivere il file: {e}"))?;
    std::fs::rename(&tmp, target).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("non riesco a scrivere il file: {e}")
    })
}

impl Worker {
    pub fn new(tx: Sender<Event>, wake: Arc<dyn Fn() + Send + Sync>) -> Worker {
        let queue = Arc::new((Mutex::new(Queue { jobs: VecDeque::new(), closed: false }), Condvar::new()));
        let q = queue.clone();
        std::thread::spawn(move || {
            let mut st = State { orig: None, cache: None };
            loop {
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
                let Some(job) = job else { continue };
                let msgs = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| run(&mut st, job, &tx)))
                    .unwrap_or_else(|p| vec![Msg::Failed(u64::MAX, super::model::panic_text(&*p))]);
                for m in msgs {
                    if tx.send(Event::PdEd(m)).is_err() {
                        return;
                    }
                }
                wake();
            }
        });
        Worker { queue }
    }

    fn push(&self, job: Job, front: bool) {
        let (lock, cv) = &*self.queue;
        let mut g = lock.lock().unwrap_or_else(|e| e.into_inner());
        if front {
            g.jobs.push_front(job);
        } else {
            g.jobs.push_back(job);
        }
        cv.notify_one();
    }

    pub fn open(&self, path: &Path) {
        let (lock, _) = &*self.queue;
        lock.lock().unwrap_or_else(|e| e.into_inner()).jobs.clear();
        self.push(Job::Open(path.to_path_buf()), false);
    }

    /// Disegni e testo di una versione: quelli in coda di una versione più vecchia si tolgono, quelli della stessa si uniscono.
    pub fn render(&self, snap: &Snap, pages: Vec<(usize, u32)>, thumbs: Vec<usize>, text: Vec<usize>) {
        let (lock, cv) = &*self.queue;
        let mut g = lock.lock().unwrap_or_else(|e| e.into_inner());
        g.jobs.retain(|j| !matches!(j, Job::Render { snap: s, .. } if s.rev < snap.rev));
        for j in g.jobs.iter_mut() {
            if let Job::Render { snap: s, pages: p, thumbs: t, text: x } = j {
                if s.rev == snap.rev {
                    for it in &pages {
                        p.retain(|(n, _)| *n != it.0);
                        p.push(*it);
                    }
                    for it in &thumbs {
                        if !t.contains(it) {
                            t.push(*it);
                        }
                    }
                    for it in &text {
                        if !x.contains(it) {
                            x.push(*it);
                        }
                    }
                    return;
                }
            }
        }
        g.jobs.push_front(Job::Render { snap: snap.clone(), pages, thumbs, text });
        cv.notify_one();
    }

    pub fn other(&self, job: Job) {
        self.push(job, false);
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        let (lock, cv) = &*self.queue;
        lock.lock().unwrap_or_else(|e| e.into_inner()).closed = true;
        cv.notify_all();
    }
}

fn run(st: &mut State, job: Job, tx: &Sender<Event>) -> Vec<Msg> {
    match job {
        Job::Open(path) => {
            let r = crate::input_file::read(&path).map_err(|e| format!("Non riesco a leggere il file: {e}")).and_then(|bytes| {
                let opened = pdfmod::open(&bytes)?;
                st.orig = Some(Arc::new(bytes));
                st.cache = None;
                Ok(Box::new(Loaded { path, opened }))
            });
            vec![Msg::Opened(r)]
        }
        Job::Release => {
            st.orig = None;
            st.cache = None;
            Vec::new()
        }
        Job::Report { snap } => match st.orig.clone() {
            Some(orig) => match pdfmod::apply_with_report(&orig, &snap.edits, &SaveOpts::default()) {
                Ok((_, r)) => vec![Msg::Reported { rev: snap.rev, glyphs: r.glyphs, images: r.images, annots: r.annots }],
                Err(e) => vec![Msg::Failed(snap.rev, e)],
            },
            None => Vec::new(),
        },
        Job::Render { snap, pages, thumbs, text } => {
            let bytes = match st.derived(&snap) {
                Ok(b) => b,
                Err(e) => return vec![Msg::Failed(snap.rev, e)],
            };
            let mut out = Vec::new();
            // Il testo prima: serve a scegliere i paragrafi anche mentre le pagine si disegnano.
            if !text.is_empty() {
                let tps = pdfmod::read_texts(&bytes, &text).unwrap_or_default();
                for (page, tp) in text.iter().zip(tps) {
                    out.push(Msg::Text { rev: snap.rev, page: *page, text: tp.map(|t| Arc::new(page_text(&t))) });
                }
            }
            for (page, side, img) in render_pages(&bytes, &pages) {
                out.push(Msg::Page { rev: snap.rev, page, side, img });
            }
            let small: Vec<(usize, u32)> = thumbs.iter().map(|p| (*p, 240)).collect();
            for (page, _, img) in render_pages(&bytes, &small) {
                out.push(Msg::Thumb { rev: snap.rev, page, img });
            }
            out
        }
        Job::Find { snap, id, query, case, whole } => {
            let hits = st.derived(&snap).and_then(|b| pdfmod::apply::find_all(&b, &query, case, whole));
            vec![Msg::Found { id, hits }]
        }
        Job::Patterns { snap, id, which } => {
            let hits = st.derived(&snap).and_then(|b| pdfmod::apply::find_patterns(&b, &which));
            vec![Msg::Patterns { id, hits }]
        }
        Job::Replace { snap, id, query, with, case, whole } => {
            let edits = st.derived(&snap).and_then(|b| replace_edits(&b, &query, &with, case, whole));
            vec![Msg::Replaced { id, edits }]
        }
        Job::Save { snap, opts, source, dir, exists } => {
            let r = (|| {
                let orig = st.orig.clone().ok_or("nessun file aperto")?;
                let target = target_for(&source, dir.as_deref(), exists, tx)?;
                let (bytes, report) = pdfmod::apply_with_report(&orig, &snap.edits, &opts)?;
                write_atomic(&target, &bytes)?;
                let mut msg = String::new();
                if report.glyphs > 0 || report.images > 0 || report.annots > 0 {
                    let mut parts = Vec::new();
                    if report.glyphs > 0 {
                        parts.push(format!("{} caratteri tolti dal testo", report.glyphs));
                    }
                    if report.images > 0 {
                        parts.push(format!("{} immagini oscurate", report.images));
                    }
                    if report.annots > 0 {
                        parts.push(format!("{} commenti o campi tolti", report.annots));
                    }
                    msg = parts.join(", ");
                }
                Ok((target, msg))
            })();
            vec![Msg::Saved(r)]
        }
    }
}
