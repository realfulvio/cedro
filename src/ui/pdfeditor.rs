//! L'editor dei PDF (scheda PDF → «Modifica»): lo stato della schermata e quello che fa. Le modifiche stanno
//! in una lista (`pdfmod::Edit`) in punti della pagina come la si vede; ogni modifica ne fa una nuova
//! versione, e il thread di fondo (`pdfedwork`) la applica a una copia del file e ne disegna le pagine.
//! L'annullamento riporta la lista com'era: niente si scrive mai sull'originale. Il disegno sta in
//! `views/pdfeditor.rs`; i clic e i tasti in `pdfedact.rs`.

use std::cell::{Cell, RefCell};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::core::R;
use super::gfx::Image;
use super::model::{App, Notice};
use super::pdfedwork::{Blk, Job, Msg, PageText, Snap};
use crate::pdfmod::{find, Align, Color, Edit, FieldKind, FontChoice, MarkKind, Opened, Rect, ShapeKind};

/// Le cinque schede in alto.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum EdTab {
    Text,
    Comments,
    Shapes,
    Forms,
    Redact,
}

impl EdTab {
    pub const ALL: [EdTab; 5] = [EdTab::Text, EdTab::Comments, EdTab::Shapes, EdTab::Forms, EdTab::Redact];

    pub fn label(self) -> &'static str {
        match self {
            EdTab::Text => "Testo",
            EdTab::Comments => "Commenti",
            EdTab::Shapes => "Immagini e forme",
            EdTab::Forms => "Moduli",
            EdTab::Redact => "Oscura",
        }
    }

    /// Gli strumenti della scheda, in gruppi (la riga sotto le schede).
    pub fn tools(self) -> &'static [&'static [Tool]] {
        match self {
            EdTab::Text => &[&[Tool::EditText, Tool::AddText], &[Tool::Find]],
            EdTab::Comments => &[&[Tool::Highlight, Tool::Underline, Tool::Strike], &[Tool::Note, Tool::Pen, Tool::Stamp]],
            EdTab::Shapes => &[&[Tool::Image], &[Tool::Rect, Tool::Ellipse, Tool::Line, Tool::Arrow]],
            EdTab::Forms => &[&[Tool::Fill], &[Tool::NewText, Tool::NewCheck, Tool::NewRadio, Tool::NewList, Tool::NewSign]],
            EdTab::Redact => &[&[Tool::Area, Tool::SearchRedact], &[Tool::Hidden]],
        }
    }

    pub fn first_tool(self) -> Tool {
        self.tools()[0][0]
    }
}

/// Lo strumento in mano.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Tool {
    Select,
    EditText,
    AddText,
    Find,
    Highlight,
    Underline,
    Strike,
    Note,
    Pen,
    Stamp,
    Image,
    Rect,
    Ellipse,
    Line,
    Arrow,
    Fill,
    NewText,
    NewCheck,
    NewRadio,
    NewList,
    NewSign,
    Area,
    SearchRedact,
    Hidden,
}

impl Tool {
    pub fn tab(self) -> EdTab {
        match self {
            Tool::Select | Tool::EditText | Tool::AddText | Tool::Find => EdTab::Text,
            Tool::Highlight | Tool::Underline | Tool::Strike | Tool::Note | Tool::Pen | Tool::Stamp => EdTab::Comments,
            Tool::Image | Tool::Rect | Tool::Ellipse | Tool::Line | Tool::Arrow => EdTab::Shapes,
            Tool::Fill | Tool::NewText | Tool::NewCheck | Tool::NewRadio | Tool::NewList | Tool::NewSign => EdTab::Forms,
            Tool::Area | Tool::SearchRedact | Tool::Hidden => EdTab::Redact,
        }
    }

    /// Il nome sul pulsante (a capo dove serve).
    pub fn label(self) -> &'static str {
        match self {
            Tool::Select => "Seleziona",
            Tool::EditText => "Modifica testo",
            Tool::AddText => "Aggiungi testo",
            Tool::Find => "Trova e sostituisci",
            Tool::Highlight => "Evidenzia",
            Tool::Underline => "Sottolinea",
            Tool::Strike => "Barra",
            Tool::Note => "Nota",
            Tool::Pen => "Matita",
            Tool::Stamp => "Timbro",
            Tool::Image => "Immagine",
            Tool::Rect => "Rettangolo",
            Tool::Ellipse => "Ellisse",
            Tool::Line => "Linea",
            Tool::Arrow => "Freccia",
            Tool::Fill => "Compila",
            Tool::NewText => "Campo di testo",
            Tool::NewCheck => "Spunta",
            Tool::NewRadio => "Scelta",
            Tool::NewList => "Elenco",
            Tool::NewSign => "Firma",
            Tool::Area => "Oscura un'area",
            Tool::SearchRedact => "Cerca e oscura",
            Tool::Hidden => "Dati nascosti",
        }
    }

    /// Cosa fa, in una riga: sotto il titolo del pannello delle proprietà.
    pub fn about(self) -> &'static str {
        match self {
            Tool::Select => "Clic su una cosa aggiunta per spostarla, cambiarla o toglierla.",
            Tool::EditText => "Clic su un paragrafo e scrivi: il testo cambia dentro il file.",
            Tool::AddText => "Clic sulla pagina (o trascina un riquadro) e scrivi un testo nuovo.",
            Tool::Find => "Cerca una parola o una frase in tutto il file e sostituiscila.",
            Tool::Highlight => "Trascina sul testo per evidenziarlo.",
            Tool::Underline => "Trascina sul testo per sottolinearlo.",
            Tool::Strike => "Trascina sul testo per barrarlo.",
            Tool::Note => "Clic sulla pagina e scrivi la nota: resta un fumetto giallo.",
            Tool::Pen => "Disegna a mano libera sulla pagina.",
            Tool::Stamp => "Scegli il timbro e mettilo sulla pagina.",
            Tool::Image => "Scegli un'immagine e mettila sulla pagina.",
            Tool::Rect | Tool::Ellipse => "Trascina sulla pagina per disegnare la forma.",
            Tool::Line | Tool::Arrow => "Trascina da un punto all'altro.",
            Tool::Fill => "Clic su un campo del modulo per compilarlo.",
            Tool::NewText | Tool::NewCheck | Tool::NewRadio | Tool::NewList | Tool::NewSign => "Trascina sulla pagina per mettere il campo.",
            Tool::Area => "Trascina un rettangolo: testo e immagini sotto spariscono davvero.",
            Tool::SearchRedact => "Scegli cosa cercare nel pannello: le parole trovate si segnano da sole.",
            Tool::Hidden => "Quello che il file porta con sé e non si vede.",
        }
    }

    /// Lo strumento che lavora con un clic o un trascinamento sulla pagina.
    pub fn on_page(self) -> bool {
        !matches!(self, Tool::Find | Tool::SearchRedact | Tool::Hidden)
    }
}

/// I timbri pronti.
pub const STAMPS: [(&str, Color); 8] = [
    ("APPROVATO", [0x1E, 0x8A, 0x55]),
    ("RESPINTO", [0xC8, 0x32, 0x3C]),
    ("BOZZA", [0x25, 0x63, 0xB8]),
    ("COPIA CONFORME", [0x25, 0x63, 0xB8]),
    ("PROTOCOLLATO", [0x66, 0x3F, 0xC0]),
    ("RISERVATO", [0xC8, 0x32, 0x3C]),
    ("URGENTE", [0xD4, 0x57, 0x0E]),
    ("ANNULLATO", [0xC8, 0x32, 0x3C]),
];

/// I colori di evidenziatore e commenti.
pub const MARK_COLORS: [Color; 5] = [[0xFF, 0xE0, 0x4A], [0x6B, 0xD9, 0x7A], [0xFF, 0x8F, 0xC0], [0x6F, 0xB6, 0xFF], [0xFF, 0xA3, 0x4D]];
/// I colori di testo, penna e forme.
pub const INK_COLORS: [Color; 5] = [[0x1D, 0x1D, 0x1F], [0x1F, 0x4E, 0x9A], [0xD6, 0x32, 0x3C], [0x1E, 0x8A, 0x55], [0xD4, 0x57, 0x0E]];

/// I caratteri che si possono scegliere per il testo nuovo (la famiglia, `None` = quello del file).
pub const FONTS: [&str; 5] = ["Carlito", "Arial", "Times New Roman", "Cambria", "Courier New"];

/// Una pagina disegnata: a che versione, e con quanti pixel per il lato lungo.
pub struct PageImg {
    pub rev: u64,
    pub side: u32,
    pub img: Image,
}

/// Quello che si sta facendo col mouse sulla pagina.
#[derive(Clone, Debug)]
pub enum Drag {
    Words { page: usize, from: (f32, f32), to: (f32, f32) },
    Rubber { page: usize, from: (f32, f32), to: (f32, f32) },
    Line { page: usize, from: (f32, f32), to: (f32, f32) },
    Pen { page: usize, pts: Vec<(f32, f32)> },
    Move { index: usize, page: usize, from: (f32, f32), orig: Edit },
}

/// Quello che sta scrivendo dentro la pagina il testo in modifica.
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    /// Il paragrafo del file che stava in questo rettangolo.
    Existing(Rect),
    /// Un testo nuovo (angolo e larghezza).
    New,
    /// Un testo aggiunto prima: la posizione nella lista.
    Again(usize),
    /// Una nota (nuova).
    Note,
    /// Una nota già messa: la posizione nella lista.
    NoteAgain(usize),
}

/// Cosa è cambiato rispetto a com'era scritto il paragrafo.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Ovr {
    pub size: Option<f32>,
    pub bold: Option<bool>,
    pub italic: Option<bool>,
    pub color: Option<Color>,
    pub align: Option<Align>,
}

#[derive(Clone, Debug)]
pub struct TextEdit {
    pub page: usize,
    pub target: Target,
    /// Dove sta sulla pagina (punti).
    pub rect: Rect,
    pub orig: String,
    pub text: String,
    pub size: f32,
    pub color: Color,
    pub bold: bool,
    pub italic: bool,
    pub align: Align,
    pub font: FontChoice,
    pub font_name: String,
    pub ovr: Ovr,
    pub has: Option<Arc<HashSet<char>>>,
}

impl TextEdit {
    /// Le lettere che il carattere del file non ha: il testo nuovo le scrive con un altro carattere.
    pub fn missing(&self) -> Vec<char> {
        let Some(has) = &self.has else { return Vec::new() };
        let mut out: Vec<char> = Vec::new();
        for c in self.text.chars().filter(|c| !c.is_whitespace() && !has.contains(c)) {
            if !out.contains(&c) {
                out.push(c);
            }
        }
        out
    }
}

/// Un campo del modulo in compilazione (testo).
#[derive(Clone, Debug)]
pub struct FieldEdit {
    pub name: String,
    pub rect: Rect,
    pub page: usize,
    pub multiline: bool,
    pub max_len: usize,
    pub text: String,
}

/// Una riga dell'elenco dei commenti.
#[derive(Clone, Debug)]
pub struct CommentRow {
    pub page: usize,
    pub kind: String,
    pub text: String,
    /// Un commento che il file aveva già, o uno aggiunto adesso (posizione nella lista).
    pub from_file: Option<(u32, u16)>,
    pub index: Option<usize>,
    pub color: Option<Color>,
}

/// Dove sono le cose sulla schermata: lo scrive il disegno, lo leggono i clic.
#[derive(Default)]
pub struct Layout {
    /// La zona della pagina che si vede (e che scorre).
    pub canvas: R,
    /// Le pagine che si vedono adesso e dove stanno sullo schermo.
    pub pages: Vec<(usize, R)>,
    /// Dove comincia ogni pagina (y dentro il contenuto, dall'alto).
    pub tops: Vec<f32>,
    /// Quanto è alto il contenuto che scorre.
    pub content_h: f32,
    /// Pixel (logici) per punto, com'è adesso.
    pub k: f32,
    /// La pagina che sta più in alto adesso.
    pub cur: usize,
    pub wants: Vec<(usize, u32)>,
    pub thumbs: Vec<usize>,
    pub texts: Vec<usize>,
    /// Il testo che si sta scrivendo: la zona sullo schermo e, per ogni riga, dove comincia e le x dei caratteri.
    pub edit_box: Option<(R, f32, Vec<(usize, Vec<f32>)>)>,
    /// La zona delle miniature.
    pub thumb_view: R,
    /// Quanto sono alti i contenuti che scorrono nel pannello (proprietà, miniature, commenti), dall'ultimo disegno.
    pub heights: [f32; 3],
    /// Di quanto la pagina è più larga della zona (per la barra di scorrimento in orizzontale).
    pub max_x: f32,
}

pub struct PdfEd {
    pub path: PathBuf,
    pub name: String,
    pub opened: Option<Opened>,
    pub edits: Vec<Edit>,
    pub undo: Vec<Vec<Edit>>,
    pub redo: Vec<Vec<Edit>>,
    pub rev: u64,
    snap: Option<Snap>,
    pub dirty: bool,
    pub saved_len: usize,
    pub tab: EdTab,
    pub tool: Tool,
    /// La scheda a sinistra: 0 pagine, 1 commenti.
    pub left: u8,
    /// Il pannello delle proprietà: tutto (`true`) o solo l'essenziale.
    pub complete: bool,
    /// L'elenco dei commenti mostra solo quelli della pagina in vista.
    pub only_page: bool,
    /// Lo zoom scelto (pixel per punto, 0 = largo quanto la zona) e le pagine una accanto all'altra.
    pub zoom: f32,
    pub two: bool,
    pub pages: HashMap<usize, PageImg>,
    pub thumbs: HashMap<usize, (u64, Image)>,
    pub texts: HashMap<usize, (u64, Arc<PageText>)>,
    pub asked: HashSet<(u64, usize, u32)>,
    pub asked_thumb: HashSet<(u64, usize)>,
    pub asked_text: HashSet<(u64, usize)>,
    // Com'è la mano: colori, grandezze, scelte.
    pub ink: Color,
    pub mark: Color,
    pub fill: Option<Color>,
    pub width: f32,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub align: Align,
    pub font: FontChoice,
    pub stamp: usize,
    pub stamp_text: String,
    pub image: Option<(Arc<Vec<u8>>, u32, u32)>,
    // Il mouse sulla pagina.
    pub drag: Option<Drag>,
    pub sel: Option<usize>,
    pub sel_file: Option<(u32, u16)>,
    pub mouse: Option<(usize, f32, f32)>,
    pub text: Option<TextEdit>,
    pub field: Option<FieldEdit>,
    // Trova e sostituisci.
    pub find: String,
    pub replace: String,
    pub case: bool,
    pub whole: bool,
    pub found: Vec<(usize, find::Hit)>,
    pub find_cur: usize,
    pub find_id: u64,
    pub finding: bool,
    pub find_done: bool,
    // Cerca e oscura.
    pub pats: Vec<(find::Pattern, Vec<(usize, Rect)>, bool)>,
    pub free: String,
    pub pat_id: u64,
    /// Cosa si sta cercando per oscurare: `Some(Some(modello))`, `Some(None)` = la parola scritta a mano.
    pub pat_which: Option<Option<find::Pattern>>,
    /// Il campo «elenco» del modulo di cui si sta scegliendo la voce.
    pub list_for: Option<usize>,
    /// Quante aree ha segnato l'ultima parola cercata a mano.
    pub free_n: Option<usize>,
    pub applied: Option<(usize, usize, usize)>,
    // Salvataggio.
    pub flatten: bool,
    pub strip: bool,
    pub options_open: bool,
    pub saving: bool,
    pub saved: Option<PathBuf>,
    pub save_msg: String,
    /// Si vuole uscire (verso la scheda PDF o un'altra attività) ma ci sono modifiche non salvate.
    pub leave: Option<Option<super::model::PdfMode>>,
    pub leave_after_save: Option<Option<super::model::PdfMode>>,
    /// Si esce verso un'altra pagina di Cedro (la barra di sinistra): quale.
    pub leave_lab: Option<super::model::Lab>,
    pub lay: RefCell<Layout>,
    pub scroll_to: Cell<Option<f32>>,
    pub busy_until: Cell<u64>,
    next_id: u64,
}

impl PdfEd {
    pub fn new(path: &Path) -> PdfEd {
        PdfEd {
            path: path.to_path_buf(),
            name: path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default(),
            opened: None,
            edits: Vec::new(),
            undo: Vec::new(),
            redo: Vec::new(),
            rev: 0,
            snap: None,
            dirty: false,
            saved_len: 0,
            tab: EdTab::Text,
            tool: Tool::EditText,
            left: 0,
            complete: true,
            only_page: false,
            zoom: 0.0,
            two: false,
            pages: HashMap::new(),
            thumbs: HashMap::new(),
            texts: HashMap::new(),
            asked: HashSet::new(),
            asked_thumb: HashSet::new(),
            asked_text: HashSet::new(),
            ink: INK_COLORS[2],
            mark: MARK_COLORS[0],
            fill: None,
            width: 2.0,
            size: 14.0,
            bold: false,
            italic: false,
            align: Align::Left,
            font: FontChoice::Same,
            stamp: 0,
            stamp_text: String::new(),
            image: None,
            drag: None,
            sel: None,
            sel_file: None,
            mouse: None,
            text: None,
            field: None,
            find: String::new(),
            replace: String::new(),
            case: false,
            whole: false,
            found: Vec::new(),
            find_cur: 0,
            find_id: 0,
            finding: false,
            find_done: false,
            pats: Vec::new(),
            free: String::new(),
            pat_id: 0,
            pat_which: None,
            list_for: None,
            free_n: None,
            applied: None,
            flatten: false,
            strip: false,
            options_open: false,
            saving: false,
            saved: None,
            save_msg: String::new(),
            leave: None,
            leave_after_save: None,
            leave_lab: None,
            lay: RefCell::new(Layout::default()),
            scroll_to: Cell::new(None),
            busy_until: Cell::new(0),
            next_id: 1,
        }
    }

    pub fn id(&mut self) -> u64 {
        self.next_id += 1;
        self.next_id
    }

    pub fn page_count(&self) -> usize {
        self.opened.as_ref().map_or(0, |o| o.sizes.len())
    }

    pub fn size_of(&self, page: usize) -> (f32, f32) {
        self.opened.as_ref().and_then(|o| o.sizes.get(page)).map_or((595.0, 842.0), |s| (s.w, s.h))
    }

    pub fn snap(&mut self) -> Snap {
        if self.snap.as_ref().is_none_or(|s| s.rev != self.rev) {
            self.snap = Some(Snap { rev: self.rev, edits: Arc::new(self.edits.clone()) });
        }
        self.snap.clone().expect("versione")
    }

    /// Quante modifiche non sono ancora salvate.
    pub fn unsaved(&self) -> usize {
        if self.dirty {
            self.edits.len().abs_diff(self.saved_len).max(1)
        } else {
            0
        }
    }

    /// Mette la lista di modifiche nuova: tiene la vecchia per annullare.
    pub fn commit(&mut self, edits: Vec<Edit>) {
        let old = std::mem::replace(&mut self.edits, edits);
        self.undo.push(old);
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
        self.bump();
    }

    pub fn push(&mut self, e: Edit) -> usize {
        let mut v = self.edits.clone();
        v.push(e);
        let at = v.len() - 1;
        self.commit(v);
        at
    }

    fn bump(&mut self) {
        self.rev += 1;
        self.dirty = true;
        self.found.clear();
        self.find_done = false;
        self.applied = None;
        self.saved = None;
        // I segni dei modelli che non ci sono più (tolti con l'annulla) non contano.
        self.pats.retain_mut(|(_, marks, _)| {
            marks.retain(|(p, r)| self.edits.iter().any(|e| matches!(e, Edit::Redact { page, rect, applied: false } if page == p && rect == r)));
            true
        });
    }

    pub fn undo(&mut self) {
        if let Some(prev) = self.undo.pop() {
            let now = std::mem::replace(&mut self.edits, prev);
            self.redo.push(now);
            self.sel = None;
            self.bump();
        }
    }

    pub fn redo(&mut self) {
        if let Some(next) = self.redo.pop() {
            let now = std::mem::replace(&mut self.edits, next);
            self.undo.push(now);
            self.sel = None;
            self.bump();
        }
    }

    /// Le aree da oscurare ancora da applicare.
    pub fn pending_redacts(&self) -> usize {
        self.edits.iter().filter(|e| matches!(e, Edit::Redact { applied: false, .. })).count()
    }

    /// I campi del modulo compilati da qui (uno per campo, anche se cambiati più volte).
    pub fn filled_fields(&self) -> usize {
        let mut names: Vec<&str> = self.edits.iter().filter_map(|e| if let Edit::FieldValue { name, .. } = e { Some(name.as_str()) } else { None }).collect();
        names.sort_unstable();
        names.dedup();
        names.len()
    }

    pub fn text_of(&self, page: usize) -> Option<&Arc<PageText>> {
        self.texts.get(&page).map(|t| &t.1)
    }

    /// Il testo di una pagina, solo se è quello della versione di adesso.
    pub fn fresh_text(&self, page: usize) -> Option<&Arc<PageText>> {
        self.texts.get(&page).filter(|t| t.0 == self.rev).map(|t| &t.1)
    }

    pub fn block_at(&self, page: usize, x: f32, y: f32) -> Option<&Blk> {
        let tp = self.fresh_text(page)?;
        // Il più piccolo che contiene il punto (un paragrafo dentro un riquadro più grande).
        tp.blocks.iter().filter(|b| b.rect.inflate(2.0).contains(x, y)).min_by(|a, b| (a.rect.w * a.rect.h).total_cmp(&(b.rect.w * b.rect.h)))
    }

    /// La parola sotto il punto, o la più vicina entro pochi punti.
    pub fn word_near(&self, page: usize, x: f32, y: f32) -> Option<usize> {
        let tp = self.fresh_text(page)?;
        let mut best: Option<(usize, f32)> = None;
        for (i, w) in tp.words.iter().enumerate() {
            let dx = if x < w.rect.x { w.rect.x - x } else if x > w.rect.right() { x - w.rect.right() } else { 0.0 };
            let dy = if y < w.rect.y { w.rect.y - y } else if y > w.rect.bottom() { y - w.rect.bottom() } else { 0.0 };
            let d = (dx * dx + dy * dy).sqrt();
            if best.is_none_or(|b| d < b.1) {
                best = Some((i, d));
            }
        }
        best.filter(|b| b.1 < 30.0).map(|b| b.0)
    }

    /// I rettangoli (uno per riga) delle parole tra due punti.
    pub fn words_between(&self, page: usize, a: (f32, f32), b: (f32, f32)) -> Vec<Rect> {
        let (Some(tp), Some(i), Some(j)) = (self.fresh_text(page), self.word_near(page, a.0, a.1), self.word_near(page, b.0, b.1)) else { return Vec::new() };
        let (i, j) = (i.min(j), i.max(j));
        let mut out: Vec<(usize, Rect)> = Vec::new();
        for w in &tp.words[i..=j] {
            match out.last_mut() {
                Some((line, r)) if *line == w.line => *r = r.union(&w.rect),
                _ => out.push((w.line, w.rect)),
            }
        }
        out.into_iter().map(|(_, r)| r).collect()
    }

    /// La modifica più in alto sotto il punto (la posizione nella lista).
    pub fn edit_at(&self, page: usize, x: f32, y: f32) -> Option<usize> {
        self.edits.iter().enumerate().rev().find_map(|(i, e)| {
            if e.page() != Some(page) {
                return None;
            }
            let hit = match e {
                Edit::Mark { rects, .. } => rects.iter().any(|r| r.inflate(2.0).contains(x, y)),
                Edit::Pen { points, width, .. } => points.windows(2).any(|w| dist_seg((x, y), w[0], w[1]) <= width / 2.0 + 4.0),
                Edit::Shape { kind: ShapeKind::Line | ShapeKind::Arrow, a, b, width, .. } => dist_seg((x, y), *a, *b) <= width / 2.0 + 4.0,
                Edit::Redact { rect, applied: true, .. } => {
                    let _ = rect;
                    false
                }
                other => other.bounds().is_some_and(|r| r.inflate(3.0).contains(x, y)),
            };
            hit.then_some(i)
        })
    }

    /// Un commento che il file aveva già, sotto il punto.
    pub fn comment_at(&self, page: usize, x: f32, y: f32) -> Option<(u32, u16)> {
        let o = self.opened.as_ref()?;
        o.comments.iter().find(|c| c.page == page && c.rect.inflate(3.0).contains(x, y) && !self.deleted(c.id)).map(|c| c.id)
    }

    pub fn deleted(&self, id: (u32, u16)) -> bool {
        self.edits.iter().any(|e| matches!(e, Edit::DeleteAnnot { id: i, .. } if *i == id))
    }

    /// Il campo del modulo sotto il punto.
    pub fn field_at(&self, page: usize, x: f32, y: f32) -> Option<usize> {
        let o = self.opened.as_ref()?;
        o.fields.iter().position(|f| f.page == page && !f.pushbutton && f.rect.inflate(1.0).contains(x, y))
    }

    /// Se un campo del modulo ha già un valore (per le spunte: acceso).
    pub fn field_filled(&self, fld: &crate::pdfmod::form::Field) -> bool {
        let v = self.field_value(&fld.name);
        match fld.kind {
            FieldKind::Check => v == "On" || (!fld.export.is_empty() && v == fld.export),
            FieldKind::Radio => !v.is_empty() && v != "Off",
            FieldKind::Sign => false,
            _ => !v.trim().is_empty(),
        }
    }

    /// Il valore di un campo: l'ultimo scritto, o quello del file.
    pub fn field_value(&self, name: &str) -> String {
        if let Some(Edit::FieldValue { value, .. }) = self.edits.iter().rev().find(|e| matches!(e, Edit::FieldValue { name: n, .. } if n == name)) {
            return value.clone();
        }
        self.opened.as_ref().and_then(|o| o.fields.iter().find(|f| f.name == name)).map(|f| f.value.clone()).unwrap_or_default()
    }

    /// Mette il valore di un campo (una sola modifica per campo).
    pub fn set_field(&mut self, name: &str, value: String) {
        let mut v: Vec<Edit> = self.edits.iter().filter(|e| !matches!(e, Edit::FieldValue { name: n, .. } if n == name)).cloned().collect();
        let original = self.opened.as_ref().and_then(|o| o.fields.iter().find(|f| f.name == name)).map(|f| f.value.clone()).unwrap_or_default();
        if value != original {
            v.push(Edit::FieldValue { name: name.to_string(), value });
        }
        self.commit(v);
    }

    /// Un nome libero per un campo nuovo.
    pub fn new_field_name(&self, kind: FieldKind) -> String {
        let base = match kind {
            FieldKind::Text => "Testo",
            FieldKind::Check => "Spunta",
            FieldKind::Radio => "Scelta",
            FieldKind::List => "Elenco",
            FieldKind::Sign => "Firma",
        };
        let used = |n: &str| {
            self.edits.iter().any(|e| matches!(e, Edit::NewField { name, .. } if name == n)) || self.opened.as_ref().is_some_and(|o| o.fields.iter().any(|f| f.name == n))
        };
        (1..).map(|i| format!("{base}{i}")).find(|n| !used(n)).unwrap_or_else(|| base.to_string())
    }

    /// I commenti del file e quelli aggiunti, nell'ordine delle pagine.
    pub fn comments(&self) -> Vec<CommentRow> {
        let mut rows = Vec::new();
        if let Some(o) = &self.opened {
            for c in o.comments.iter().filter(|c| !self.deleted(c.id)) {
                rows.push(CommentRow { page: c.page, kind: c.kind.clone(), text: c.text.clone(), from_file: Some(c.id), index: None, color: None });
            }
        }
        for (i, e) in self.edits.iter().enumerate() {
            let (kind, text, color) = match e {
                Edit::Mark { kind, note, color, .. } => (
                    match kind {
                        MarkKind::Highlight => "Evidenzia",
                        MarkKind::Underline => "Sottolinea",
                        MarkKind::Strike => "Barra",
                    },
                    note.clone(),
                    *color,
                ),
                Edit::Note { text, color, .. } => ("Nota", text.clone(), *color),
                Edit::Pen { color, .. } => ("Matita", String::new(), *color),
                Edit::Stamp { label, color, .. } => ("Timbro", label.clone(), *color),
                Edit::Shape { kind, color, .. } => (
                    match kind {
                        ShapeKind::Rect => "Rettangolo",
                        ShapeKind::Ellipse => "Ellisse",
                        ShapeKind::Line => "Linea",
                        ShapeKind::Arrow => "Freccia",
                    },
                    String::new(),
                    *color,
                ),
                _ => continue,
            };
            rows.push(CommentRow { page: e.page().unwrap_or(0), kind: kind.to_string(), text, from_file: None, index: Some(i), color: Some(color) });
        }
        rows.sort_by_key(|r| r.page);
        rows
    }

    /// Quante modifiche per pagina (il segno sulle miniature).
    pub fn per_page(&self) -> HashMap<usize, usize> {
        let mut m = HashMap::new();
        for e in &self.edits {
            if let Some(p) = e.page() {
                *m.entry(p).or_insert(0) += 1;
            }
        }
        m
    }

    /// Il testo della modifica scelta (nota, timbro, commento di un evidenziatore).
    pub fn sel_text(&self) -> Option<String> {
        match self.edits.get(self.sel?)? {
            Edit::Note { text, .. } => Some(text.clone()),
            Edit::Mark { note, .. } => Some(note.clone()),
            Edit::Stamp { label, .. } => Some(label.clone()),
            _ => None,
        }
    }

    pub fn set_sel_text(&mut self, text: String) {
        let Some(i) = self.sel else { return };
        match self.edits.get_mut(i) {
            Some(Edit::Note { text: t, .. }) | Some(Edit::Mark { note: t, .. }) | Some(Edit::Stamp { label: t, .. }) => *t = text,
            _ => return,
        }
        // Scrivere in un campo non fa una versione per lettera: il disegno si rifà con la prossima.
        self.rev += 1;
        self.dirty = true;
    }

    /// Il colore della modifica scelta.
    pub fn sel_color(&self) -> Option<Color> {
        match self.edits.get(self.sel?)? {
            Edit::Mark { color, .. } | Edit::Note { color, .. } | Edit::Pen { color, .. } | Edit::Stamp { color, .. } | Edit::Shape { color, .. } | Edit::AddText { color, .. } => Some(*color),
            _ => None,
        }
    }

    pub fn recolor(&mut self, c: Color) {
        let Some(i) = self.sel else { return };
        let mut v = self.edits.clone();
        match v.get_mut(i) {
            Some(Edit::Mark { color, .. }) | Some(Edit::Note { color, .. }) | Some(Edit::Pen { color, .. }) | Some(Edit::Stamp { color, .. }) | Some(Edit::Shape { color, .. }) | Some(Edit::AddText { color, .. }) => *color = c,
            _ => return,
        }
        self.commit(v);
    }

    /// Toglie la modifica scelta (o il commento del file scelto).
    pub fn delete_selected(&mut self) {
        if let Some(i) = self.sel.take() {
            if i < self.edits.len() {
                let mut v = self.edits.clone();
                v.remove(i);
                self.commit(v);
            }
        } else if let Some(id) = self.sel_file.take() {
            let page = self.opened.as_ref().and_then(|o| o.comments.iter().find(|c| c.id == id)).map_or(0, |c| c.page);
            self.push(Edit::DeleteAnnot { page, id });
        }
    }

    /// Toglie una riga dell'elenco dei commenti.
    pub fn delete_comment(&mut self, row: &CommentRow) {
        if let Some(i) = row.index {
            if i < self.edits.len() {
                let mut v = self.edits.clone();
                v.remove(i);
                self.sel = None;
                self.commit(v);
            }
        } else if let Some(id) = row.from_file {
            self.sel_file = None;
            self.push(Edit::DeleteAnnot { page: row.page, id });
        }
    }

    /// Cosa dice la riga sopra la pagina.
    pub fn banner(&self) -> String {
        if let Some(t) = &self.text {
            return match t.target {
                Target::Note | Target::NoteAgain(_) => "Scrivi la nota. Invio va a capo, clic fuori per finire, Esc per annullare.".into(),
                _ => "Stai scrivendo dentro il file. Invio va a capo, clic fuori per finire, Esc per annullare.".into(),
            };
        }
        if self.field.is_some() {
            return "Scrivi nel campo. Invio o clic fuori per finire, Esc per annullare.".into();
        }
        if let Some((glyphs, images, _)) = self.applied {
            return match (glyphs, images) {
                (0, 0) => "Oscurato: niente di leggibile restava sotto le aree.".into(),
                (1, 0) => "1 parola cancellata dal file: non si recupera né copiando né cercando.".into(),
                (g, 0) => format!("{g} parole cancellate dal file: non si recuperano né copiando né cercando."),
                (0, 1) => "1 immagine oscurata dal file.".into(),
                (0, i) => format!("{i} immagini oscurate dal file."),
                (g, i) => format!("{} e {} tolte dal file.", plural(g, "parola", "parole"), plural(i, "immagine", "immagini")),
            };
        }
        self.tool.about().to_string()
    }

    /// La riga di stato in fondo: «3 pagine · 6 modifiche non salvate».
    pub fn status(&self) -> String {
        let (pages, changes) = self.status_parts();
        format!("{pages} · {changes}")
    }

    /// Le due metà della riga di stato, da tradurre una per una: «3 pagine» e «6 modifiche non salvate».
    pub fn status_parts(&self) -> (String, String) {
        let pages = plural(self.page_count(), "pagina", "pagine");
        let changes = match self.unsaved() {
            0 => "nessuna modifica".to_string(),
            1 => "1 modifica non salvata".to_string(),
            k => format!("{k} modifiche non salvate"),
        };
        (pages, changes)
    }

    /// Il nome del file che esce.
    pub fn out_name(&self) -> String {
        let stem = self.path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
        format!("{stem}-modificato.pdf")
    }
}

fn plural(n: usize, one: &str, many: &str) -> String {
    if n == 1 {
        format!("1 {one}")
    } else {
        format!("{n} {many}")
    }
}

fn dist_seg(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 <= 0.0 { 0.0 } else { (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0) };
    let (qx, qy) = (a.0 + t * dx, a.1 + t * dy);
    ((p.0 - qx).powi(2) + (p.1 - qy).powi(2)).sqrt()
}

/// Sposta una modifica di (dx, dy) punti.
pub fn translate(e: &mut Edit, dx: f32, dy: f32) {
    match e {
        Edit::Mark { rects, .. } => {
            for r in rects {
                r.x += dx;
                r.y += dy;
            }
        }
        Edit::Note { x, y, .. } | Edit::AddText { x, y, .. } => {
            *x += dx;
            *y += dy;
        }
        Edit::Pen { points, .. } => {
            for p in points {
                p.0 += dx;
                p.1 += dy;
            }
        }
        Edit::Stamp { rect, .. } | Edit::Image { rect, .. } | Edit::NewField { rect, .. } | Edit::Redact { rect, applied: false, .. } | Edit::EditText { rect, .. } => {
            rect.x += dx;
            rect.y += dy;
        }
        Edit::Shape { a, b, .. } => {
            a.0 += dx;
            a.1 += dy;
            b.0 += dx;
            b.1 += dy;
        }
        _ => {}
    }
}

impl App {
    /// Apre l'editor sul PDF `path`.
    pub fn ed_open(&mut self, path: &Path) {
        if self.busy() {
            self.notice = Some(Notice { text: "Aspetta che finisca il lavoro in corso.".into(), error: true });
            return;
        }
        self.notice = None;
        self.ed = Some(Box::new(PdfEd::new(path)));
        self.edw.open(path);
    }

    /// Chiude l'editor (le modifiche non salvate si perdono: chi chiama ha già chiesto).
    pub fn ed_close(&mut self) {
        if self.ed.take().is_some() {
            self.edw.other(Job::Release);
        }
    }

    pub fn ed_msg(&mut self, m: Msg) {
        let Some(ed) = self.ed.as_mut() else { return };
        match m {
            Msg::Opened(Ok(l)) => {
                if l.path == ed.path {
                    ed.opened = Some(l.opened);
                }
            }
            Msg::Opened(Err(e)) => {
                self.ed = None;
                self.notice = Some(Notice { text: e, error: true });
            }
            Msg::Page { rev, page, side, img } => {
                if let Some(img) = img {
                    if ed.pages.get(&page).is_none_or(|p| p.rev <= rev) {
                        ed.pages.insert(page, PageImg { rev, side, img });
                    }
                }
            }
            Msg::Thumb { rev, page, img } => {
                if let Some(img) = img {
                    if ed.thumbs.get(&page).is_none_or(|p| p.0 <= rev) {
                        ed.thumbs.insert(page, (rev, img));
                    }
                }
            }
            Msg::Text { rev, page, text } => {
                if let Some(t) = text {
                    if ed.texts.get(&page).is_none_or(|p| p.0 <= rev) {
                        ed.texts.insert(page, (rev, t));
                    }
                }
            }
            Msg::Failed(rev, e) => {
                // L'ultima modifica non si può applicare: si toglie e si dice perché.
                if (rev == ed.rev || rev == u64::MAX) && !ed.undo.is_empty() {
                    ed.undo();
                    ed.redo.clear();
                }
                self.notice = Some(Notice { text: format!("Non riesco a fare questa modifica: {e}"), error: true });
            }
            Msg::Found { id, hits } if id == ed.pat_id && id != ed.find_id => {
                ed.pat_which = None;
                match hits {
                    Ok(h) => ed.mark_hits(None, h),
                    Err(e) => self.notice = Some(Notice { text: e, error: true }),
                }
            }
            Msg::Found { id, hits } => {
                if id == ed.find_id {
                    ed.finding = false;
                    ed.find_done = true;
                    match hits {
                        Ok(h) => {
                            ed.found = h;
                            ed.find_cur = 0;
                            if let Some((p, hit)) = ed.found.first() {
                                let y = hit.rects.first().map_or(0.0, |r| r.y);
                                ed.goto(*p, y);
                            }
                        }
                        Err(e) => self.notice = Some(Notice { text: e, error: true }),
                    }
                }
            }
            Msg::Patterns { id, hits } => {
                if id == ed.pat_id {
                    let which = ed.pat_which.take().flatten();
                    match hits {
                        Ok(h) => ed.mark_hits(which, h.into_iter().map(|(p, _, hit)| (p, hit)).collect()),
                        Err(e) => self.notice = Some(Notice { text: e, error: true }),
                    }
                }
            }
            Msg::Replaced { id, edits } => {
                if id == ed.find_id {
                    ed.finding = false;
                    match edits {
                        Ok((new, n)) if n > 0 => {
                            let mut v = ed.edits.clone();
                            v.extend(new);
                            ed.commit(v);
                            ed.found.clear();
                            self.notice = Some(Notice { text: format!("Sostituite {n} occorrenze."), error: false });
                        }
                        Ok(_) => self.notice = Some(Notice { text: "Non ho trovato niente da sostituire.".into(), error: false }),
                        Err(e) => self.notice = Some(Notice { text: e, error: true }),
                    }
                }
            }
            Msg::Reported { rev, glyphs, images, annots } => {
                if rev == ed.rev {
                    ed.applied = Some((glyphs, images, annots));
                }
            }
            Msg::Saved(r) => {
                ed.saving = false;
                match r {
                    Ok((path, msg)) => {
                        ed.dirty = false;
                        ed.saved_len = ed.edits.len();
                        ed.saved = Some(path.clone());
                        ed.save_msg = msg;
                        let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        self.notice = Some(Notice { text: format!("Salvato: {}", crate::i18n::lit(&name)), error: false });
                        if let Some(leave) = ed.leave_after_save.take() {
                            self.ed_leave(leave);
                        }
                    }
                    Err(e) => {
                        ed.leave_after_save = None;
                        self.notice = Some(Notice { text: e, error: true });
                    }
                }
            }
        }
    }

    /// Esce dall'editor verso la scheda PDF (o un altro modo del PDF).
    pub fn ed_leave(&mut self, mode: Option<super::model::PdfMode>) {
        let lab = self.ed.as_ref().and_then(|e| e.leave_lab);
        self.ed_close();
        let simple = self.simple_view;
        if let Some(l) = lab {
            self.go_lab(l, simple);
        } else if let Some(m) = mode {
            self.set_pdf_mode(m, simple);
        }
    }

    /// Va a una pagina della barra di sinistra del Cedro di partenza (come il clic sulla barra).
    pub fn go_lab(&mut self, lab: super::model::Lab, simple: bool) {
        use super::model::Lab;
        self.notice = None;
        if lab != self.lab {
            self.set_lab(lab, simple);
        }
        if lab == Lab::Converti && self.kind() == crate::engine::Kind::Pdf {
            self.cedro_leave_pdf(simple);
        }
        if lab == Lab::Pdf {
            self.cedro_back_to_pdf();
        }
    }

    /// Dopo il disegno: chiede al thread di fondo le pagine, le miniature e il testo che servono e non ci sono.
    pub fn ed_flush(&mut self) {
        let Some(ed) = self.ed.as_mut() else { return };
        if ed.opened.is_none() {
            return;
        }
        let lay = std::mem::take(&mut *ed.lay.borrow_mut());
        let rev = ed.rev;
        let mut pages = Vec::new();
        for (p, side) in &lay.wants {
            if ed.asked.insert((rev, *p, *side)) {
                pages.push((*p, *side));
            }
        }
        let mut thumbs = Vec::new();
        for p in &lay.thumbs {
            if ed.thumbs.get(p).is_none_or(|t| t.0 != rev) && ed.asked_thumb.insert((rev, *p)) {
                thumbs.push(*p);
            }
        }
        let mut texts = Vec::new();
        for p in &lay.texts {
            if ed.texts.get(p).is_none_or(|t| t.0 != rev) && ed.asked_text.insert((rev, *p)) {
                texts.push(*p);
            }
        }
        // Lo stato del disegno resta quello dell'ultimo fotogramma (lo rimette il prossimo).
        let keep = Layout { canvas: lay.canvas, pages: lay.pages.clone(), tops: lay.tops.clone(), content_h: lay.content_h, k: lay.k, cur: lay.cur, thumb_view: lay.thumb_view, edit_box: lay.edit_box.clone(), heights: lay.heights, max_x: lay.max_x, ..Layout::default() };
        *ed.lay.borrow_mut() = keep;
        if pages.is_empty() && thumbs.is_empty() && texts.is_empty() {
            return;
        }
        let snap = ed.snap();
        self.edw.render(&snap, pages, thumbs, texts);
    }

    /// Salva una copia con tutte le modifiche (`leave`: poi si esce).
    pub fn ed_save(&mut self, leave: Option<Option<super::model::PdfMode>>) {
        let dest = self.dest.clone();
        let exists = self.values.exists;
        let Some(ed) = self.ed.as_mut() else { return };
        if ed.saving {
            return;
        }
        ed.saving = true;
        ed.leave_after_save = leave;
        let opts = crate::pdfmod::SaveOpts { flatten: ed.flatten, strip: ed.strip };
        let snap = ed.snap();
        let source = ed.path.clone();
        self.edw.other(Job::Save { snap, opts, source, dir: dest, exists });
    }
}

impl PdfEd {
    /// Porta la pagina `page` a quella y (punti) in cima alla zona.
    pub fn goto(&self, page: usize, y_pt: f32) {
        let lay = self.lay.borrow();
        if let Some(top) = lay.tops.get(page) {
            self.scroll_to.set(Some((top + (y_pt * lay.k - 80.0).max(0.0)).max(0.0)));
        } else {
            self.scroll_to.set(Some(f32::from(page as u16) * 800.0));
        }
    }

    /// Le cose trovate che diventano aree da oscurare (un'unica versione): `which` è il modello cercato,
    /// `None` la parola scritta a mano.
    pub fn mark_hits(&mut self, which: Option<find::Pattern>, hits: Vec<(usize, find::Hit)>) {
        let mut v = self.edits.clone();
        let mut marks: Vec<(usize, Rect)> = Vec::new();
        let mut added = false;
        for (page, h) in hits {
            for r in h.rects {
                let r = r.inflate(1.0);
                if !v.iter().any(|e| matches!(e, Edit::Redact { page: pg, rect, applied: false } if *pg == page && *rect == r)) {
                    v.push(Edit::Redact { page, rect: r, applied: false });
                    added = true;
                }
                marks.push((page, r));
            }
        }
        if added {
            self.commit(v);
        }
        match which {
            Some(p) => {
                self.pats.retain(|e| e.0 != p);
                self.pats.push((p, marks, true));
            }
            None => self.free_n = Some(marks.len()),
        }
    }

    /// Le aree segnate per un modello (quante).
    pub fn marked(&self, p: find::Pattern) -> Option<usize> {
        self.pats.iter().find(|e| e.0 == p && e.2).map(|e| e.1.len())
    }
}
