//! I temi della versione standard: come sono disposte le finestre e con quali colori.
//! Il logo Spicchio è lo stesso in tutti; cambiano disposizione e palette (kit «temi e logo»).
//! La modalità (Standard o Semplificata) è un'altra scelta: è l'impostazione `simple` di sempre.

use super::gfx::Rgb;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ThemeId {
    WindowsEssenziale,
    PercorsoGuidato,
    CedroCompatto,
    BancoDiLavoro,
    DocumentoAlCentro,
}

pub const THEMES: [ThemeId; 5] = [ThemeId::WindowsEssenziale, ThemeId::PercorsoGuidato, ThemeId::CedroCompatto, ThemeId::BancoDiLavoro, ThemeId::DocumentoAlCentro];

impl ThemeId {
    /// Il nome scritto in config.txt (`ui_theme=`).
    pub fn key(self) -> &'static str {
        match self {
            ThemeId::WindowsEssenziale => "windows-essenziale",
            ThemeId::PercorsoGuidato => "percorso-guidato",
            ThemeId::CedroCompatto => "cedro-compatto",
            ThemeId::BancoDiLavoro => "banco-di-lavoro",
            ThemeId::DocumentoAlCentro => "documento-al-centro",
        }
    }

    /// Un valore sconosciuto (o vuoto) torna a Windows essenziale: l'avvio non si blocca mai.
    pub fn from_key(key: &str) -> ThemeId {
        THEMES.into_iter().find(|t| t.key() == key.trim()).unwrap_or(ThemeId::WindowsEssenziale)
    }

    pub fn name(self) -> &'static str {
        match self {
            ThemeId::WindowsEssenziale => "Windows essenziale",
            ThemeId::PercorsoGuidato => "Percorso guidato",
            ThemeId::CedroCompatto => "Cedro compatto",
            ThemeId::BancoDiLavoro => "Banco di lavoro",
            ThemeId::DocumentoAlCentro => "Documento al centro",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            ThemeId::WindowsEssenziale => "Attività a sinistra, coda con ispettore.",
            ThemeId::PercorsoGuidato => "Tre passi: File, Operazione, Salvataggio.",
            ThemeId::CedroCompatto => "Finestrella dall'icona vicino all'orologio.",
            ThemeId::BancoDiLavoro => "Tabella con stati, ispettore a destra.",
            ThemeId::DocumentoAlCentro => "L'originale al centro, opzioni accanto.",
        }
    }

    pub fn palette(self) -> &'static Palette {
        match self {
            ThemeId::WindowsEssenziale => &ESSENZIALE,
            ThemeId::PercorsoGuidato => &GUIDATO,
            ThemeId::CedroCompatto => &COMPATTO,
            ThemeId::BancoDiLavoro => &BANCO,
            ThemeId::DocumentoAlCentro => &DOCUMENTO,
        }
    }

    /// Il tema scuro vuole il logo con la scritta chiara.
    pub fn dark(self) -> bool {
        matches!(self, ThemeId::CedroCompatto | ThemeId::BancoDiLavoro)
    }
}

/// I colori di un tema (design-system.json del kit), più due tinte derivate usate dalle tavole.
pub struct Palette {
    pub bg: Rgb,
    pub surface: Rgb,
    pub alt: Rgb,
    pub text: Rgb,
    pub muted: Rgb,
    pub primary: Rgb,
    pub on_primary: Rgb,
    pub border: Rgb,
    pub ok: Rgb,
    pub bad: Rgb,
    /// Fondo della zona di lavoro dentro la cornice (appena diverso da `bg`).
    pub work: Rgb,
    /// Voce scelta nella barra di sinistra.
    pub nav_sel: Rgb,
}

const fn hex(v: u32) -> Rgb {
    ((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub const ESSENZIALE: Palette = Palette {
    bg: hex(0xF5F8FC),
    surface: hex(0xFFFFFF),
    alt: hex(0xEAF2F9),
    text: hex(0x172B3A),
    muted: hex(0x586B80),
    primary: hex(0x005E8E),
    on_primary: hex(0xFFFFFF),
    border: hex(0xD5E0EC),
    ok: hex(0x1D7A46),
    bad: hex(0xB3261E),
    work: hex(0xFAFCFE),
    nav_sel: hex(0xDCEAF7),
};

pub const GUIDATO: Palette = Palette {
    bg: hex(0xFAF8F3),
    surface: hex(0xFFFEFC),
    alt: hex(0xFFF1E5),
    text: hex(0x142D2C),
    muted: hex(0x687582),
    primary: hex(0xED620C),
    on_primary: hex(0xFFFFFF),
    border: hex(0xE6E1D9),
    ok: hex(0x3F7356),
    bad: hex(0xB3261E),
    work: hex(0xF1EEE8),
    nav_sel: hex(0xFFE9D7),
};

pub const COMPATTO: Palette = Palette {
    bg: hex(0x191F25),
    surface: hex(0x20272D),
    alt: hex(0x29323A),
    text: hex(0xF6F8FA),
    muted: hex(0xB7C1C9),
    primary: hex(0x70EDB4),
    on_primary: hex(0x10271D),
    border: hex(0x3F4B55),
    ok: hex(0x70EDB4),
    bad: hex(0xFF9A9A),
    work: hex(0x161B20),
    nav_sel: hex(0x29323A),
};

pub const BANCO: Palette = Palette {
    bg: hex(0x101D29),
    surface: hex(0x162636),
    alt: hex(0x20374C),
    text: hex(0xF1F6FF),
    muted: hex(0xB5C4D5),
    primary: hex(0x00B8E6),
    on_primary: hex(0x09222D),
    border: hex(0x385269),
    ok: hex(0x13CC7C),
    bad: hex(0xFF999F),
    work: hex(0x0C1722),
    nav_sel: hex(0x1B3247),
};

pub const DOCUMENTO: Palette = Palette {
    bg: hex(0xF7F5F9),
    surface: hex(0xFFFFFF),
    alt: hex(0xF0EAF5),
    text: hex(0x241A31),
    muted: hex(0x6C6578),
    primary: hex(0x663079),
    on_primary: hex(0xFFFFFF),
    border: hex(0xE1DAE7),
    ok: hex(0x2F805C),
    bad: hex(0xAF2543),
    work: hex(0xEDE9F1),
    nav_sel: hex(0xF0EAF5),
};

/// Colori del marchio (uguali in tutti i temi).
pub const BRAND_PRIMARY: Rgb = hex(0x793FCB);
pub const BRAND_ACCENT: Rgb = hex(0x14845B);
pub const BRAND_WORD: Rgb = hex(0x30213E);

/// Mescola due colori: `k` = 0 dà `a`, 1 dà `b`.
pub fn mix(a: Rgb, b: Rgb, k: f32) -> Rgb {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * k).round() as u8;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_round_trip_and_unknown_falls_back() {
        for t in THEMES {
            assert_eq!(ThemeId::from_key(t.key()), t);
        }
        assert_eq!(ThemeId::from_key("ruota-e-dettagli"), ThemeId::WindowsEssenziale);
        assert_eq!(ThemeId::from_key(""), ThemeId::WindowsEssenziale);
    }
}
