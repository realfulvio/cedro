//! Le pagine impaginate di un documento o di un foglio: testo, riquadri, linee e immagini in punti,
//! con l'origine in alto a sinistra. Le scrive il PDF; l'anteprima le guarda dal PDF.

use super::font::FontId;
use crate::pdfdoc::DocImage;

/// Un colore RGB.
pub type Rgb = (u8, u8, u8);

pub const BLACK: Rgb = (0, 0, 0);

#[derive(Clone, Debug)]
pub enum Op {
    /// Un pezzo di testo con la linea di base a `y`.
    Text { x: f32, y: f32, font: FontId, size: f32, color: Rgb, text: String },
    Rect { x: f32, y: f32, w: f32, h: f32, color: Rgb },
    Line { x1: f32, y1: f32, x2: f32, y2: f32, width: f32, color: Rgb, dash: Dash },
    /// Un'immagine di `Stampato::images`.
    Image { index: usize, x: f32, y: f32, w: f32, h: f32 },
    /// Da qui in poi si disegna solo dentro il rettangolo, fino a `Unclip`.
    Clip { x: f32, y: f32, w: f32, h: f32 },
    Unclip,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dash {
    Solid,
    Dashed,
    Dotted,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Uri(String),
    /// Un segnalibro del documento (il nome).
    Anchor(String),
}

#[derive(Clone, Debug)]
pub struct Link {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub target: Target,
}

/// Un segnalibro del PDF: un titolo del documento o il nome di un foglio.
#[derive(Clone, Debug)]
pub struct Mark {
    pub level: u8,
    pub title: String,
    pub y: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Pagina {
    pub w: f32,
    pub h: f32,
    pub ops: Vec<Op>,
    pub links: Vec<Link>,
    pub marks: Vec<Mark>,
    /// I punti di arrivo dei link interni: nome e altezza.
    pub anchors: Vec<(String, f32)>,
}

impl Pagina {
    pub fn new(w: f32, h: f32) -> Pagina {
        Pagina { w, h, ..Default::default() }
    }
}

/// Un documento impaginato, pronto per il PDF.
pub struct Stampato {
    pub pages: Vec<Pagina>,
    pub images: Vec<DocImage>,
    pub fonts: super::font::Fonti,
    pub title: String,
}
