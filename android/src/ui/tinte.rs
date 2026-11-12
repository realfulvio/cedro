//! Le quattro tinte del Cedro di partenza (Melanzana, Notte, Bosco, Carta) sul telefono: la cornice, le scritte
//! e il pannello chiaro del disegno approvato, e la palette che ne ricavano le altre schermate (Lavori, Aspetto,
//! Opzioni, PDF, Firma…) per stare nella stessa cornice. I valori sono quelli del computer (`theme::Tint`).

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

use cedro::ui::gfx::Rgb;
use cedro::ui::theme::{mix, Palette, ThemeId, Tint};

/// Le quattro tinte di Cedro, nell'ordine in cui si scelgono.
pub const CEDRO: [ThemeId; 4] = [ThemeId::Cedro, ThemeId::CedroNotte, ThemeId::CedroBosco, ThemeId::CedroCarta];

pub fn nome(t: ThemeId) -> &'static str {
    match t {
        ThemeId::CedroNotte => "Notte",
        ThemeId::CedroBosco => "Bosco",
        ThemeId::CedroCarta => "Carta",
        _ => "Cedro",
    }
}

/// Il tema è una delle quattro tinte di Cedro.
pub fn e_cedro(t: ThemeId) -> bool {
    t.is_cedro()
}

/// Colori scuri (cornice scura) o chiari: per le tinte di Cedro lo dice la tinta, per gli altri temi il tema.
pub fn scuro(t: ThemeId) -> bool {
    if t.is_cedro() {
        !t.tint().light
    } else {
        t.dark()
    }
}

/// Il pannello chiaro che sta sotto il file (uguale in tutte le tinte, con la sua variante fredda per Notte e Bosco).
pub struct Pannello {
    pub sf: Rgb,
    pub alt: Rgb,
    pub ink: Rgb,
    pub mut_: Rgb,
    pub line: Rgb,
}

pub fn pannello(t: ThemeId) -> Pannello {
    match t {
        ThemeId::CedroNotte => Pannello { sf: (255, 255, 255), alt: (0xEE, 0xF3, 0xF8), ink: (0x17, 0x2B, 0x3A), mut_: (0x58, 0x6B, 0x80), line: (0xD9, 0xE2, 0xEC) },
        ThemeId::CedroBosco => Pannello { sf: (255, 255, 255), alt: (0xEE, 0xF5, 0xF1), ink: (0x16, 0x30, 0x2A), mut_: (0x58, 0x6F, 0x66), line: (0xD8, 0xE5, 0xDE) },
        _ => Pannello { sf: (255, 255, 255), alt: (0xF3, 0xEF, 0xF8), ink: (0x2B, 0x1F, 0x38), mut_: (0x6A, 0x5E, 0x79), line: (0xE2, 0xDC, 0xEA) },
    }
}

fn da_tinta(t: &Tint) -> Palette {
    let surf = if t.light { (255, 255, 255) } else { mix(t.frame, (255, 255, 255), 0.07) };
    Palette {
        bg: t.frame,
        surface: surf,
        alt: if t.light { mix(t.frame, (255, 255, 255), 0.5) } else { mix(t.frame, (255, 255, 255), 0.11) },
        text: t.ink,
        muted: t.muted,
        primary: t.brand,
        on_primary: if t.light { (255, 255, 255) } else { (0x15, 0x10, 0x1C) },
        border: t.line,
        ok: t.leaf,
        bad: if t.light { (0xB3, 0x26, 0x1E) } else { (0xFF, 0x8A, 0x8F) },
        work: t.frame,
        nav_sel: t.line,
    }
}

/// La palette delle altre schermate: per le tinte di Cedro quella della cornice, per gli altri temi la loro.
pub fn palette(t: ThemeId) -> &'static Palette {
    static P: OnceLock<[Palette; 4]> = OnceLock::new();
    if !t.is_cedro() {
        return t.palette();
    }
    let p = P.get_or_init(|| CEDRO.map(|x| da_tinta(x.tint())));
    &p[CEDRO.iter().position(|x| *x == t).unwrap_or(0)]
}

/// La palette del pannello chiaro con `accento` come colore principale (il colore della pagina). Se ne fanno
/// poche (quattro tinte per sei colori): si tengono, una volta fatte.
pub fn palette_pannello(t: ThemeId, accento: Rgb) -> &'static Palette {
    static C: OnceLock<Mutex<HashMap<(ThemeId, Rgb), &'static Palette>>> = OnceLock::new();
    let mut m = C.get_or_init(|| Mutex::new(HashMap::new())).lock().unwrap_or_else(|e| e.into_inner());
    m.entry((t, accento)).or_insert_with(|| {
        let q = pannello(t);
        Box::leak(Box::new(Palette {
            bg: q.sf,
            surface: q.alt,
            alt: q.line,
            text: q.ink,
            muted: q.mut_,
            primary: accento,
            on_primary: (255, 255, 255),
            border: q.line,
            ok: (0x14, 0x84, 0x5B),
            bad: (0xB3, 0x26, 0x1E),
            work: q.sf,
            nav_sel: q.alt,
        }))
    })
}
