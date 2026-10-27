//! I colori e le misure delle finestre di Cedro (Impostazioni, guida, ruota della guida): due set, quello
//! «Limone» (dal logo real>hcok_) e quello del Comune di Bondeno (palette del sito del Comune), scelti dal
//! tema in Impostazioni › Aspetto. Windows e Linux leggono da qui.

/// Un colore come (r, g, b).
pub type Rgb = (u8, u8, u8);

pub struct Tema {
    /// Nome del carattere dell'interfaccia (incorporato in assets/fonts).
    pub font: &'static str,
    /// Raggio degli angoli di riquadri e pulsanti, in px di progetto.
    pub radius: i32,

    // Finestre
    pub bg: Rgb,
    pub fg: Rgb,
    pub mute: Rgb,
    pub line: Rgb,
    /// Fondo di elenchi, campi e righe delle impostazioni.
    pub surf: Rgb,
    /// Barra di sinistra: fondo, testo, voce scelta e il suo testo.
    pub side: Rgb,
    pub side_fg: Rgb,
    pub side_sel: Rgb,
    pub side_sel_fg: Rgb,
    /// Pulsante principale.
    pub acc: Rgb,
    pub acc_fg: Rgb,
    /// Formato, scheda o levetta scelta.
    pub sel: Rgb,
    pub sel_fg: Rgb,
    /// Zona dove si lasciano i file.
    pub drop: Rgb,
    pub drop_line: Rgb,
    pub link: Rgb,
    pub ok: Rgb,
    pub fail: Rgb,

    // Ruota
    pub slice: Rgb,
    pub slice_fg: Rgb,
    pub hover: Rgb,
    pub hover_fg: Rgb,
    /// Filo attorno alle fette e al centro (con la sua opacità, 0..1).
    pub edge: Rgb,
    pub edge_alpha: f32,
    /// Il disco al centro e le sue tre righe: titolo, formato, nome del file.
    pub hole: Rgb,
    pub hole_ring: Rgb,
    pub hole_title: Rgb,
    pub hole_main: Rgb,
    pub hole_sub: Rgb,
    /// Spazio tra le fette e arrotondamento dei loro angoli.
    pub half_gap: f32,
    pub corner: f32,
    /// Titolo al centro della ruota in maiuscolo («CONVERSIONE») invece che «Conversione».
    pub maiuscole: bool,
}

impl Tema {
    /// Un titolo breve (centro della ruota, intestazioni) scritto come vuole il tema.
    pub fn titolo(&self, text: &str) -> String {
        if self.maiuscole {
            text.to_uppercase()
        } else {
            text.to_string()
        }
    }
}

/// Inchiostro blu-nero e giallo limone, come il logo.
pub const LIMONE: Tema = Tema {
    font: "Figtree",
    radius: 10,
    bg: (0xFF, 0xFF, 0xFF),
    fg: (0x0F, 0x17, 0x1D),
    mute: (0x5A, 0x6B, 0x78),
    line: (0xE2, 0xE5, 0xE7),
    surf: (0xF5, 0xF6, 0xF7),
    side: (0x0F, 0x17, 0x1D),
    side_fg: (0xAE, 0xBA, 0xC3),
    side_sel: (0xF2, 0xC2, 0x30),
    side_sel_fg: (0x0F, 0x17, 0x1D),
    acc: (0x0F, 0x17, 0x1D),
    acc_fg: (0xF2, 0xC2, 0x30),
    sel: (0xF2, 0xC2, 0x30),
    sel_fg: (0x0F, 0x17, 0x1D),
    drop: (0xFF, 0xF9, 0xE0),
    drop_line: (0xE2, 0xB5, 0x24),
    link: (0x00, 0x5E, 0x8E),
    ok: (0x1D, 0x7A, 0x46),
    fail: (0xB3, 0x26, 0x1E),
    slice: (0xFF, 0xFF, 0xFF),
    slice_fg: (0x0F, 0x17, 0x1D),
    hover: (0xF2, 0xC2, 0x30),
    hover_fg: (0x0F, 0x17, 0x1D),
    edge: (0x0F, 0x17, 0x1D),
    edge_alpha: 0.18,
    hole: (0x0F, 0x17, 0x1D),
    hole_ring: (0x0F, 0x17, 0x1D),
    hole_title: (0xF2, 0xC2, 0x30),
    hole_main: (0xFF, 0xFF, 0xFF),
    hole_sub: (0xC9, 0xD3, 0xDA),
    half_gap: 0.0,
    corner: 0.0,
    maiuscole: false,
};

/// Tema Comune di Bondeno: i colori del sito del Comune, fette arrotondate e staccate.
pub const BONDENO: Tema = Tema {
    font: "Titillium Web",
    radius: 0,
    bg: (0xFF, 0xFF, 0xFF),
    fg: (0x19, 0x19, 0x19),
    mute: (0x45, 0x5A, 0x64),
    line: (0xD6, 0xDE, 0xE5),
    surf: (0xFF, 0xFF, 0xFF),
    side: (0xF3, 0xF6, 0xF9),
    side_fg: (0x19, 0x19, 0x19),
    side_sel: (0x00, 0x5E, 0x8E),
    side_sel_fg: (0xFF, 0xFF, 0xFF),
    acc: (0x00, 0x5E, 0x8E),
    acc_fg: (0xFF, 0xFF, 0xFF),
    sel: (0x00, 0x5E, 0x8E),
    sel_fg: (0xFF, 0xFF, 0xFF),
    drop: (0xE3, 0xF0, 0xF8),
    drop_line: (0x00, 0x5E, 0x8E),
    link: (0x00, 0x66, 0xCC),
    ok: (0x1D, 0x7A, 0x46),
    fail: (0xB3, 0x26, 0x1E),
    slice: (0xEB, 0xEE, 0xF0),
    slice_fg: (0x19, 0x19, 0x19),
    hover: (0x00, 0x66, 0xCC),
    hover_fg: (0xFF, 0xFF, 0xFF),
    edge: (0x00, 0x00, 0x00),
    edge_alpha: 0.0,
    hole: (0xFF, 0xFF, 0xFF),
    hole_ring: (0x00, 0x5E, 0x8E),
    hole_title: (0x45, 0x5A, 0x64),
    hole_main: (0x00, 0x1A, 0x4D),
    hole_sub: (0x45, 0x5A, 0x64),
    half_gap: 3.0,
    corner: 7.0,
    maiuscole: true,
};

/// I colori in uso: quelli del Comune con il tema Bondeno, altrimenti i Limone.
pub fn attivo() -> &'static Tema {
    if crate::edizione::bondeno() {
        &BONDENO
    } else {
        &LIMONE
    }
}
