//! I temi della versione standard: come sono disposte le finestre e con quali colori.
//! Il logo Spicchio è lo stesso in tutti; cambiano disposizione e palette (kit «temi e logo»).
//! La vista (Semplice o Completa) è un'altra scelta: è l'impostazione `simple` di sempre.

use super::gfx::Rgb;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum ThemeId {
    /// Il Cedro di partenza (2026): cornice scura melanzana, attività a sinistra con le icone a gradiente,
    /// il file grande al centro sopra un alone dei suoi colori e un pannello chiaro con tutti i comandi.
    /// `CedroNotte`, `CedroBosco` e `CedroCarta` sono la stessa disposizione con un'altra tinta.
    Cedro,
    CedroNotte,
    CedroBosco,
    CedroCarta,
    WindowsEssenziale,
    PercorsoGuidato,
    CedroCompatto,
    BancoDiLavoro,
    DocumentoAlCentro,
    /// Comune di Bondeno: la disposizione di «Windows essenziale» con i colori e il carattere del sito del
    /// Comune e lo stemma accanto al logo (e in Impostazioni, guida, menu dell'icona).
    Bondeno,
    /// Cedro 4 Labs: la disposizione di «Windows essenziale» pensata per i laboratori (biotech, ricerca,
    /// veterinaria) e il loro ufficio: colori da bancone, bersagli grandi, niente YouTube né video dal web.
    CedroLabs,
}

pub const THEMES: [ThemeId; 11] = [
    ThemeId::Cedro,
    ThemeId::CedroNotte,
    ThemeId::CedroBosco,
    ThemeId::CedroCarta,
    ThemeId::WindowsEssenziale,
    ThemeId::PercorsoGuidato,
    ThemeId::CedroCompatto,
    ThemeId::BancoDiLavoro,
    ThemeId::DocumentoAlCentro,
    ThemeId::Bondeno,
    ThemeId::CedroLabs,
];

impl ThemeId {
    /// Il nome scritto in config.txt (`ui_theme=`).
    pub fn key(self) -> &'static str {
        match self {
            ThemeId::Cedro => "cedro",
            ThemeId::CedroNotte => "cedro-notte",
            ThemeId::CedroBosco => "cedro-bosco",
            ThemeId::CedroCarta => "cedro-carta",
            ThemeId::WindowsEssenziale => "windows-essenziale",
            ThemeId::PercorsoGuidato => "percorso-guidato",
            ThemeId::CedroCompatto => "cedro-compatto",
            ThemeId::BancoDiLavoro => "banco-di-lavoro",
            ThemeId::DocumentoAlCentro => "documento-al-centro",
            ThemeId::Bondeno => crate::edizione::CHIAVE_TEMA,
            ThemeId::CedroLabs => "cedro-labs",
        }
    }

    /// Un valore sconosciuto (o vuoto) è il Cedro di partenza: l'avvio non si blocca mai.
    pub fn from_key(key: &str) -> ThemeId {
        THEMES.into_iter().find(|t| t.key() == key.trim()).unwrap_or(ThemeId::Cedro)
    }

    /// Il Cedro di partenza, in una delle sue quattro tinte.
    pub fn is_cedro(self) -> bool {
        matches!(self, ThemeId::Cedro | ThemeId::CedroNotte | ThemeId::CedroBosco | ThemeId::CedroCarta)
    }

    /// Le disposizioni a pagine (la barra con le icone a gradiente, il file al centro, il pannello a destra).
    pub fn paged(self) -> bool {
        self.is_cedro() || self == ThemeId::CedroLabs
    }

    /// I colori della cornice di questo tema (solo per le disposizioni a pagine).
    pub fn tint(self) -> &'static Tint {
        match self {
            ThemeId::CedroNotte => &TINT_NOTTE,
            ThemeId::CedroBosco => &TINT_BOSCO,
            ThemeId::CedroCarta => &TINT_CARTA,
            _ => &TINT_MELANZANA,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            ThemeId::Cedro => "Cedro · Melanzana",
            ThemeId::CedroNotte => "Cedro · Notte",
            ThemeId::CedroBosco => "Cedro · Bosco",
            ThemeId::CedroCarta => "Cedro · Carta",
            ThemeId::WindowsEssenziale => "Windows essenziale",
            ThemeId::PercorsoGuidato => "Percorso guidato",
            ThemeId::CedroCompatto => "Cedro compatto",
            ThemeId::BancoDiLavoro => "Banco di lavoro",
            ThemeId::DocumentoAlCentro => "Documento al centro",
            ThemeId::Bondeno => "Comune di Bondeno",
            ThemeId::CedroLabs => "Cedro 4 Labs",
        }
    }

    pub fn description(self) -> &'static str {
        match self {
            ThemeId::Cedro => "Il file al centro, tutti i comandi accanto.",
            ThemeId::CedroNotte => "Lo stesso Cedro, con la cornice blu notte.",
            ThemeId::CedroBosco => "Lo stesso Cedro, con la cornice verde bosco.",
            ThemeId::CedroCarta => "Lo stesso Cedro, con la cornice chiara.",
            ThemeId::WindowsEssenziale => "Attività a sinistra, coda con ispettore.",
            ThemeId::PercorsoGuidato => "Tre passi: File, Operazione, Salvataggio.",
            ThemeId::CedroCompatto => "Finestrella dall'icona vicino all'orologio.",
            ThemeId::BancoDiLavoro => "Tabella con stati, ispettore a destra.",
            ThemeId::DocumentoAlCentro => "L'originale al centro, opzioni accanto.",
            ThemeId::Bondeno => "Colori e carattere del Comune, con lo stemma.",
            ThemeId::CedroLabs => "Per laboratori e ricerca: solo gli strumenti utili.",
        }
    }

    pub fn palette(self) -> &'static Palette {
        match self {
            ThemeId::Cedro | ThemeId::CedroCarta => &CEDRO,
            ThemeId::CedroNotte => &CEDRO_NOTTE,
            ThemeId::CedroBosco => &CEDRO_BOSCO,
            ThemeId::WindowsEssenziale => &ESSENZIALE,
            ThemeId::PercorsoGuidato => &GUIDATO,
            ThemeId::CedroCompatto => &COMPATTO,
            ThemeId::BancoDiLavoro => &BANCO,
            ThemeId::DocumentoAlCentro => &DOCUMENTO,
            ThemeId::Bondeno => &BONDENO,
            ThemeId::CedroLabs => &LABS,
        }
    }

    /// Le attività che il tema mostra nella barra: Cedro 4 Labs non ha YouTube né i video dal web
    /// (in laboratorio non servono); il programma li ha sempre, tornando a un altro tema si rivedono.
    pub fn activities(self) -> &'static [super::model::Activity] {
        use super::model::Activity;
        const LABS: [Activity; 3] = [Activity::Convert, Activity::Transcribe, Activity::Firma];
        match self {
            ThemeId::CedroLabs => &LABS,
            _ => &Activity::ALL,
        }
    }

    /// Se il tema ha YouTube e i video dal web (Cedro 4 Labs no: la Guida e la scelta della vista li saltano).
    pub fn has_web(self) -> bool {
        self != ThemeId::CedroLabs
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

/// I colori del sito del Comune di Bondeno (gli stessi di `tema::BONDENO`, usati da Impostazioni e guida).
pub const BONDENO: Palette = Palette {
    bg: hex(0xF3F6F9),
    surface: hex(0xFFFFFF),
    alt: hex(0xE3F0F8),
    text: hex(0x191919),
    muted: hex(0x455A64),
    primary: hex(0x005E8E),
    on_primary: hex(0xFFFFFF),
    border: hex(0xD6DEE5),
    ok: hex(0x1D7A46),
    bad: hex(0xB3261E),
    work: hex(0xFFFFFF),
    nav_sel: hex(0xD9E9F4),
};

/// Cedro 4 Labs: i colori del pannello chiaro (comandi, Impostazioni, Guida). La cornice scura
/// con il file sul suo alone la disegna `views::labs` con i colori qui sotto.
pub const LABS: Palette = Palette {
    bg: hex(0xF5F2F9),
    surface: hex(0xFFFFFF),
    alt: hex(0xF3EFF8),
    text: hex(0x30213E),
    muted: hex(0x6A5E79),
    primary: hex(0x793FCB),
    on_primary: hex(0xFFFFFF),
    border: hex(0xE2DCEA),
    ok: hex(0x14845B),
    bad: hex(0xB3261E),
    work: hex(0xFAF8FC),
    nav_sel: hex(0xE9E0F7),
};

/// Il Cedro di partenza: i colori del pannello chiaro dei comandi (e di Impostazioni e Guida).
pub const CEDRO: Palette = Palette {
    bg: hex(0xF5F2F9),
    surface: hex(0xFFFFFF),
    alt: hex(0xF3EFF8),
    text: hex(0x2B1F38),
    muted: hex(0x6A5E79),
    primary: hex(0x793FCB),
    on_primary: hex(0xFFFFFF),
    border: hex(0xE2DCEA),
    ok: hex(0x14845B),
    bad: hex(0xB3261E),
    work: hex(0xFAF8FC),
    nav_sel: hex(0xE9E0F7),
};

pub const CEDRO_NOTTE: Palette = Palette {
    bg: hex(0xF3F6FA),
    surface: hex(0xFFFFFF),
    alt: hex(0xEEF3F8),
    text: hex(0x172B3A),
    muted: hex(0x586B80),
    primary: hex(0x2563B8),
    on_primary: hex(0xFFFFFF),
    border: hex(0xD9E2EC),
    ok: hex(0x14845B),
    bad: hex(0xB3261E),
    work: hex(0xF8FAFC),
    nav_sel: hex(0xDCEAF7),
};

pub const CEDRO_BOSCO: Palette = Palette {
    bg: hex(0xF2F7F4),
    surface: hex(0xFFFFFF),
    alt: hex(0xEEF5F1),
    text: hex(0x16302A),
    muted: hex(0x586F66),
    primary: hex(0x14845B),
    on_primary: hex(0xFFFFFF),
    border: hex(0xD8E5DE),
    ok: hex(0x14845B),
    bad: hex(0xB3261E),
    work: hex(0xF7FAF8),
    nav_sel: hex(0xDCEFE5),
};

/// La cornice di una disposizione a pagine: fondo (con la sfumatura in alto a sinistra), scritte,
/// righe, il verde e il viola del marchio, il fondo della zona del file e il velo sotto il mouse.
pub struct Tint {
    pub frame: Rgb,
    pub frame_hi: Rgb,
    pub line: Rgb,
    pub ink: Rgb,
    pub muted: Rgb,
    /// Le scritte della voce scelta nella barra.
    pub sel: Rgb,
    pub brand: Rgb,
    pub leaf: Rgb,
    pub stage: Rgb,
    /// Il velo sotto il mouse: bianco sulle cornici scure, viola scuro su quella chiara.
    pub hover: Rgb,
    pub light: bool,
}

pub const TINT_MELANZANA: Tint = Tint {
    frame: hex(0x1B1325),
    frame_hi: hex(0x2B1D3B),
    line: hex(0x3A2E4C),
    ink: hex(0xF4EFFA),
    muted: hex(0xB9ADCA),
    sel: hex(0xFFFFFF),
    brand: hex(0xB08CF5),
    leaf: hex(0x45D39A),
    stage: hex(0x09070C),
    hover: hex(0xFFFFFF),
    light: false,
};

pub const TINT_NOTTE: Tint = Tint {
    frame: hex(0x0E1622),
    frame_hi: hex(0x17293D),
    line: hex(0x2A3D52),
    ink: hex(0xEEF4FB),
    muted: hex(0xA9B8C9),
    sel: hex(0xFFFFFF),
    brand: hex(0xB79CF5),
    leaf: hex(0x45D39A),
    stage: hex(0x06090D),
    hover: hex(0xFFFFFF),
    light: false,
};

pub const TINT_BOSCO: Tint = Tint {
    frame: hex(0x0E1A15),
    frame_hi: hex(0x17322A),
    line: hex(0x2A463C),
    ink: hex(0xEEF8F3),
    muted: hex(0xA8C2B6),
    sel: hex(0xFFFFFF),
    brand: hex(0xB79CF5),
    leaf: hex(0x45D39A),
    stage: hex(0x060A08),
    hover: hex(0xFFFFFF),
    light: false,
};

pub const TINT_CARTA: Tint = Tint {
    frame: hex(0xEEE9F4),
    frame_hi: hex(0xFBF9FD),
    line: hex(0xD6CDE2),
    ink: hex(0x2B1F38),
    muted: hex(0x6A5E79),
    sel: hex(0x2B1F38),
    brand: hex(0x793FCB),
    leaf: hex(0x14845B),
    stage: hex(0x1A1622),
    hover: hex(0x3C1E5A),
    light: true,
};

/// I colori dei comandi disegnati sulla zona scura del file (barra, elenchi, pagine del PDF): così i
/// pezzi comuni (campi, elenchi, pagine) si leggono anche lì.
pub const STAGE: Palette = Palette {
    bg: hex(0x09070C),
    surface: hex(0x1A1522),
    alt: hex(0x2A2236),
    text: hex(0xF4EFFA),
    muted: hex(0xB9ADCA),
    primary: hex(0xB08CF5),
    on_primary: hex(0x15101C),
    border: hex(0x3A2E4C),
    ok: hex(0x5BE0A3),
    bad: hex(0xFF8A8F),
    work: hex(0x120E18),
    nav_sel: hex(0x2A2236),
};

/// I due colori di una pagina del Cedro di partenza (pieno e acceso), come `lab_colors` per i laboratori.
pub fn page_colors(lab: super::model::Lab) -> (Rgb, Rgb) {
    use super::model::Lab;
    match lab {
        Lab::Home => (hex(0x793FCB), hex(0xB08CF5)),
        Lab::Pdf => (hex(0xC8323C), hex(0xFF6B78)),
        Lab::Trascrivi => (hex(0xD4570E), hex(0xFF8A3D)),
        Lab::Video => (hex(0x1E8A55), hex(0x3DCB8A)),
        Lab::Firma => (hex(0x5B4BD1), hex(0x9A8CF8)),
        _ => (hex(0x2563B8), hex(0x4C9BF5)),
    }
}

/// Il colore acceso di un tipo di file (il pallino delle etichette, la sfumatura delle tessere).
pub fn kind_bright(kind: crate::engine::Kind) -> Rgb {
    use crate::engine::Kind;
    match kind {
        Kind::Image => hex(0x3CCB9A),
        Kind::Doc => hex(0x5B9BF5),
        Kind::Pdf => hex(0xFF6B60),
        Kind::Text => hex(0x35C6D2),
        Kind::Audio => hex(0xFF8A3D),
        Kind::Video => hex(0xA982EE),
        Kind::Archive | Kind::Other => hex(0xF2B631),
    }
}

/// La cornice di Cedro 4 Labs: melanzana scuro, scritte chiare, il verde e il viola del marchio schiariti.
pub const LABS_FRAME: Rgb = hex(0x1B1325);
pub const LABS_FRAME_HI: Rgb = hex(0x281D36);
pub const LABS_INK: Rgb = hex(0xF4EFFA);
pub const LABS_MUTED: Rgb = hex(0xB9ADCA);
pub const LABS_LINE: Rgb = hex(0x3A2E4C);
pub const LABS_LEAF: Rgb = hex(0x45D39A);
pub const LABS_STAGE: Rgb = hex(0x09070C);

/// I due colori di una pagina di Cedro 4 Labs: quello pieno (pulsante, scelta attiva) e quello
/// acceso che fa la sfumatura dell'icona e il bagliore sulla cornice scura.
pub fn lab_colors(lab: super::model::Lab) -> (Rgb, Rgb) {
    use super::model::Lab;
    match lab {
        Lab::Home => (hex(0x793FCB), hex(0xB08CF5)),
        Lab::Immagini => (hex(0x0072B2), hex(0x3FA9F5)),
        Lab::Anonimizza => (hex(0xC24F00), hex(0xFF8A3D)),
        Lab::Figura => (hex(0x008564), hex(0x2FCB9A)),
        Lab::Dati => (hex(0xA66D00), hex(0xF2B631)),
        Lab::Referti => (hex(0xB4447F), hex(0xF06BB0)),
        Lab::Trascrivi => (hex(0x7B4FC9), hex(0xA982EE)),
        Lab::Converti => (hex(0x0E7C86), hex(0x35C6D2)),
        Lab::Pdf => (hex(0xC8323C), hex(0xFF6B78)),
        Lab::Video => (hex(0x1E8A55), hex(0x3DCB8A)),
        Lab::Firma => (hex(0x5B4BD1), hex(0x9A8CF8)),
    }
}

/// L'etichetta «4 LABS» accanto al logo: il magenta dei laboratori, un colore che nessun altro tema usa.
pub const LABS_BADGE: Rgb = hex(0x9C2F73);

/// Colori del marchio (uguali in tutti i temi).
pub const BRAND_PRIMARY: Rgb = hex(0x793FCB);
pub const BRAND_ACCENT: Rgb = hex(0x14845B);
pub const BRAND_WORD: Rgb = hex(0x30213E);

/// I colori delle voci riconosciute nella trascrizione: fissi, uguali in tutti i temi chiari (sono
/// anche quelli del Word) e più accesi nei temi scuri.
pub const VOICES_LIGHT: [Rgb; 8] = [hex(0x2563B8), hex(0xD4570E), hex(0x1E8A55), hex(0xB0308F), hex(0xA87A0C), hex(0x0D7C8F), hex(0xC8323C), hex(0x5B4BD1)];
pub const VOICES_DARK: [Rgb; 8] = [hex(0x7FB2FF), hex(0xFFA36B), hex(0x62D99A), hex(0xF38AD6), hex(0xF2C75A), hex(0x5AD3E6), hex(0xFF8A8F), hex(0xA99CFF)];

/// Il colore della voce `i` nel tema.
pub fn voice(theme: ThemeId, i: usize) -> Rgb {
    let set = if theme.dark() { &VOICES_DARK } else { &VOICES_LIGHT };
    set[i % set.len()]
}

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
        assert_eq!(ThemeId::from_key("ruota-e-dettagli"), ThemeId::Cedro);
        assert_eq!(ThemeId::from_key(""), ThemeId::Cedro);
        assert!(THEMES.iter().filter(|t| t.is_cedro()).count() == 4);
    }

    #[test]
    fn labs_has_no_download_activities_and_the_others_keep_all() {
        use crate::ui::model::Activity;
        assert!(!ThemeId::CedroLabs.activities().contains(&Activity::YouTube));
        assert!(!ThemeId::CedroLabs.activities().contains(&Activity::Web));
        assert!(ThemeId::CedroLabs.activities().contains(&Activity::Convert));
        assert_eq!(ThemeId::Bondeno.activities().len(), Activity::ALL.len());
        assert_eq!(ThemeId::from_key("cedro-labs"), ThemeId::CedroLabs);
    }

    #[test]
    fn voice_colors_match_the_word_ones() {
        for (i, c) in VOICES_LIGHT.iter().enumerate() {
            assert_eq!(format!("{:02X}{:02X}{:02X}", c.0, c.1, c.2), crate::trascrizione::VOICE_COLORS[i]);
        }
    }
}

/// Il colore di ogni tipo di file (schede delle categorie, zona dove si lasciano i file): gli stessi
/// toni delle icone disegnate (PDF rosso, Word blu, immagine verde…). Li usa il tema base.
pub fn kind_color(kind: crate::engine::Kind) -> Rgb {
    use crate::engine::Kind;
    match kind {
        Kind::Image => hex(0x1F8A62),
        Kind::Doc => hex(0x2B6FD3),
        Kind::Pdf => hex(0xD2382C),
        Kind::Text => hex(0x0D7C8F),
        Kind::Audio => hex(0xD4570E),
        Kind::Video => hex(0x7A3FC8),
        Kind::Archive | Kind::Other => hex(0xA2760A),
    }
}

/// Il colore di ogni attività nella barra di sinistra del tema base.
pub fn activity_color(a: super::model::Activity) -> Rgb {
    use super::model::Activity;
    match a {
        Activity::Convert => hex(0x2563B8),
        Activity::Transcribe => hex(0xD4570E),
        Activity::YouTube => hex(0xC8323C),
        Activity::Web => hex(0x1E8A55),
        Activity::Firma => hex(0x5B4BD1),
    }
}
