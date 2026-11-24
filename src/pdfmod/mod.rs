//! L'editor dei PDF: modificare il testo, commentare, evidenziare, oscurare, compilare i moduli, aggiungere
//! immagini e forme. Tutto sta in una lista di modifiche (`Edit`) in punti della pagina come la si vede
//! (origine in alto a sinistra, la pagina già girata e ritagliata): la schermata le aggiunge e le toglie,
//! `apply` le scrive in una copia del PDF. Il file originale non si tocca mai.
//!
//! - `text`: legge il testo con le sue posizioni (parole, righe, paragrafi) e lo toglie o lo sostituisce
//!   dentro i flussi della pagina: serve alla modifica del testo e all'oscuramento vero.
//! - `annot`: commenti, evidenziatore, forme, timbri, immagini e testi aggiunti, tutti con l'aspetto già
//!   disegnato (come li vedono tutti i lettori).
//! - `form`: i campi dei moduli (AcroForm) letti, compilati e nuovi.
//! - `redact`: l'oscuramento che toglie davvero testo e pixel, i dati nascosti, l'appiattimento.

pub mod annot;
pub mod apply;
pub mod find;
pub mod fonts;
pub mod form;
pub mod geom;
pub mod newtext;
pub mod redact;
pub mod text;

use std::sync::Arc;

pub use apply::{apply, apply_with_report, open, read_text, Opened, SaveOpts};
pub use text::{Align, TextBlock, TextPage, Word};

/// Un rettangolo in punti, origine in alto a sinistra della pagina vista.
#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

impl Rect {
    pub fn new(x: f32, y: f32, w: f32, h: f32) -> Rect {
        Rect { x, y, w, h }
    }

    /// Il rettangolo tra due angoli, in qualunque ordine.
    pub fn between(a: (f32, f32), b: (f32, f32)) -> Rect {
        Rect { x: a.0.min(b.0), y: a.1.min(b.1), w: (a.0 - b.0).abs(), h: (a.1 - b.1).abs() }
    }

    pub fn right(&self) -> f32 {
        self.x + self.w
    }

    pub fn bottom(&self) -> f32 {
        self.y + self.h
    }

    pub fn contains(&self, px: f32, py: f32) -> bool {
        px >= self.x && px <= self.right() && py >= self.y && py <= self.bottom()
    }

    pub fn intersects(&self, o: &Rect) -> bool {
        self.x < o.right() && o.x < self.right() && self.y < o.bottom() && o.y < self.bottom()
    }

    pub fn inflate(&self, d: f32) -> Rect {
        Rect { x: self.x - d, y: self.y - d, w: self.w + 2.0 * d, h: self.h + 2.0 * d }
    }

    pub fn union(&self, o: &Rect) -> Rect {
        let x = self.x.min(o.x);
        let y = self.y.min(o.y);
        Rect { x, y, w: self.right().max(o.right()) - x, h: self.bottom().max(o.bottom()) - y }
    }

    pub fn center(&self) -> (f32, f32) {
        (self.x + self.w / 2.0, self.y + self.h / 2.0)
    }
}

pub type Color = [u8; 3];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MarkKind {
    Highlight,
    Underline,
    Strike,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeKind {
    Rect,
    Ellipse,
    Line,
    Arrow,
}

/// Il tipo di un campo nuovo del modulo.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FieldKind {
    Text,
    Check,
    Radio,
    /// Elenco a discesa.
    List,
    /// Campo firma vuoto (si firma poi in «Firma digitale»).
    Sign,
}

/// Il carattere con cui si scrive un testo nuovo.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FontChoice {
    /// Quello del testo che c'era, se ha tutte le lettere; altrimenti Carlito (o il suo sostituto).
    Same,
    /// Una famiglia del sistema (Carlito, Titillium Web, Figtree, Courier New…).
    Family(String),
}

/// Una modifica. Le pagine sono da 0; le misure in punti della pagina vista.
#[derive(Clone, Debug)]
pub enum Edit {
    /// Evidenziatore, sottolineato o barrato: un rettangolo per riga di testo.
    Mark { page: usize, kind: MarkKind, rects: Vec<Rect>, color: Color, note: String },
    /// Una nota (il fumetto giallo) con il suo testo.
    Note { page: usize, x: f32, y: f32, text: String, color: Color },
    /// Matita: un tratto a mano libera.
    Pen { page: usize, points: Vec<(f32, f32)>, color: Color, width: f32 },
    /// Timbro («APPROVATO»…): il riquadro con il testo dentro.
    Stamp { page: usize, rect: Rect, label: String, color: Color },
    /// Rettangolo, ellisse, linea o freccia tra due punti.
    Shape { page: usize, kind: ShapeKind, a: (f32, f32), b: (f32, f32), color: Color, fill: Option<Color>, width: f32 },
    /// Un'immagine (il file PNG o JPEG com'è) dentro il rettangolo.
    Image { page: usize, rect: Rect, data: Arc<Vec<u8>> },
    /// Testo nuovo: l'angolo in alto a sinistra, la larghezza a cui andare a capo (0 = su una riga).
    AddText { page: usize, x: f32, y: f32, w: f32, text: String, size: f32, color: Color, bold: bool, italic: bool, font: FontChoice, align: Align },
    /// Il paragrafo che stava in `rect` diventa `text` (a capo con '\n' tra i capoversi).
    EditText { page: usize, rect: Rect, text: String, size: Option<f32>, bold: Option<bool>, italic: Option<bool>, color: Option<Color>, font: FontChoice, align: Option<Align> },
    /// Il valore di un campo del modulo: testo; per una spunta «On» o «Off»; per un tondino il valore scelto.
    FieldValue { name: String, value: String },
    /// Un campo nuovo del modulo.
    NewField { page: usize, rect: Rect, kind: FieldKind, name: String, options: Vec<String> },
    /// Un'area da oscurare; `applied` = già applicata (testo e pixel tolti davvero, riquadro nero).
    Redact { page: usize, rect: Rect, applied: bool },
    /// Toglie un commento che il file aveva già (oggetto della pagina).
    DeleteAnnot { page: usize, id: (u32, u16) },
}

impl Edit {
    /// La pagina su cui sta la modifica (i campi del modulo e le altre senza pagina: `None`).
    pub fn page(&self) -> Option<usize> {
        match self {
            Edit::Mark { page, .. }
            | Edit::Note { page, .. }
            | Edit::Pen { page, .. }
            | Edit::Stamp { page, .. }
            | Edit::Shape { page, .. }
            | Edit::Image { page, .. }
            | Edit::AddText { page, .. }
            | Edit::EditText { page, .. }
            | Edit::NewField { page, .. }
            | Edit::Redact { page, .. }
            | Edit::DeleteAnnot { page, .. } => Some(*page),
            Edit::FieldValue { .. } => None,
        }
    }

    /// Il rettangolo che occupa sulla pagina (per selezionarla e spostarla).
    pub fn bounds(&self) -> Option<Rect> {
        match self {
            Edit::Mark { rects, .. } => rects.iter().copied().reduce(|a, b| a.union(&b)),
            Edit::Note { x, y, .. } => Some(Rect::new(*x, *y, 20.0, 20.0)),
            Edit::Pen { points, width, .. } => {
                let mut it = points.iter();
                let first = it.next()?;
                let mut r = Rect::new(first.0, first.1, 0.0, 0.0);
                for p in it {
                    r = r.union(&Rect::new(p.0, p.1, 0.0, 0.0));
                }
                Some(r.inflate(width / 2.0))
            }
            Edit::Stamp { rect, .. } | Edit::Image { rect, .. } | Edit::NewField { rect, .. } | Edit::Redact { rect, .. } => Some(*rect),
            Edit::Shape { a, b, width, .. } => Some(Rect::between(*a, *b).inflate(width / 2.0)),
            Edit::AddText { x, y, w, size, text, .. } => {
                let lines = text.lines().count().max(1) as f32;
                let widest = text.lines().map(|l| crate::pdfedit::text_width(l, false) / 1000.0 * size).fold(0.0, f32::max);
                Some(Rect::new(*x, *y, if *w > 0.0 { *w } else { widest.max(*size) }, lines * size * newtext::LEADING))
            }
            Edit::EditText { rect, .. } => Some(*rect),
            _ => None,
        }
    }

    /// È un commento (compare nell'elenco «Commenti»).
    pub fn is_comment(&self) -> bool {
        matches!(self, Edit::Mark { .. } | Edit::Note { .. } | Edit::Pen { .. } | Edit::Stamp { .. } | Edit::Shape { .. })
    }
}

/// Quanto è grande una pagina come la si vede.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PageSize {
    pub w: f32,
    pub h: f32,
}

/// Un commento che il file aveva già.
#[derive(Clone, Debug, PartialEq)]
pub struct Comment {
    pub page: usize,
    pub id: (u32, u16),
    /// «Evidenzia», «Nota», «Sottolinea»…
    pub kind: String,
    pub rect: Rect,
    pub text: String,
    pub author: String,
}

/// Cosa porta con sé il file e non si vede: autore, programma, dati XMP, allegati.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Hidden {
    pub author: String,
    pub creator: String,
    pub producer: String,
    pub title: String,
    pub xmp: bool,
    pub attachments: usize,
    pub javascript: bool,
}

impl Hidden {
    pub fn any(&self) -> bool {
        !self.author.is_empty() || !self.creator.is_empty() || !self.producer.is_empty() || self.xmp || self.attachments > 0 || self.javascript
    }
}
