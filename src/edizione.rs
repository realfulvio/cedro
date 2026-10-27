//! Il tema «Comune di Bondeno»: uno dei temi di Impostazioni › Aspetto, con i colori e il carattere del Comune
//! e lo stemma in più punti. Motore, funzioni e pacchetti sono gli stessi della versione standard.
//! Lo stemma è l'originale del sito del Comune (assets/bondeno/FONTE.md): si mostra così com'è, mai ritoccato.

use std::sync::atomic::{AtomicBool, Ordering};

static BONDENO: AtomicBool = AtomicBool::new(false);

/// Vero quando è scelto il tema «Comune di Bondeno» (Impostazioni › Aspetto): stemma, colori e carattere del Comune.
/// Lo accendono `read_settings` e `write_settings`, così segue sempre l'impostazione `ui_theme`.
pub fn bondeno() -> bool {
    BONDENO.load(Ordering::Relaxed)
}

/// La chiave di `ui_theme` del tema Bondeno.
pub const CHIAVE_TEMA: &str = "bondeno";

/// Chiamata da `settings` ogni volta che legge o scrive l'impostazione del tema.
pub(crate) fn segui_tema(ui_theme: &str) {
    BONDENO.store(e_bondeno(ui_theme), Ordering::Relaxed);
}

fn e_bondeno(ui_theme: &str) -> bool {
    ui_theme.trim() == CHIAVE_TEMA
}

/// Il nome del tema da mostrare accanto alla versione; niente se non è quello di Bondeno.
pub fn nome() -> Option<&'static str> {
    bondeno().then_some(NOME)
}

const NOME: &str = "Comune di Bondeno";

/// L'icona dell'app (menu, area di notifica, Informazioni su Linux): lo Spicchio, con ogni tema.
/// Su Windows la icona sta nell'exe (build.rs la prende da `assets/cedro.ico`).
pub const ICONA: &[u8] = include_bytes!("../assets/icon-256.png");

/// Gli stemmi stanno sempre nel programma e si disegnano solo con il tema Bondeno.
///
/// Solo lo stemma, a colori, su fondo trasparente (PNG 300×300).
pub const STEMMA: &[u8] = include_bytes!("../assets/bondeno/Stemma-Comune-di-Bondeno.png");
/// Lo stemma con la scritta «Città di Bondeno» dentro il cerchio, su fondo trasparente.
/// Il PNG del sito ha 50 pixel di margine vuoto per lato: chi lo disegna ritaglia [`SCRITTA_RITAGLIO`].
pub const SCRITTA: &[u8] = include_bytes!("../assets/bondeno/Stemma-del-Comune-di-Bondeno-1.png");
/// Il quadrato utile di [`SCRITTA`]: x, y, lato (il resto è trasparente).
pub const SCRITTA_RITAGLIO: (u32, u32, u32) = (50, 50, 200);
/// Lo stemma con scritta in un cerchio bianco pieno: si legge sulla barra blu della guida.
pub const CERCHIO: &[u8] = include_bytes!("../assets/bondeno/Stemma-del-Comune-di-Bondeno-3.png");

/// Quale dei tre stemmi.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stemma {
    /// Solo lo stemma a colori: menu, piè di pagina, menu dell'icona.
    Solo,
    /// Con la scritta «Città di Bondeno»: Informazioni.
    Scritta,
    /// Con la scritta, nel cerchio bianco: sulla barra blu della guida.
    Cerchio,
}

/// Uno stemma decodificato, pronto da disegnare: pixel BGRA premoltiplicati, riga dopo riga.
/// È il formato di cairo (`ARgb32`) e di GDI+ (`PixelFormat32bppPARGB`) sui processori little-endian.
pub struct Immagine {
    pub w: u32,
    pub h: u32,
    pub bgra: Vec<u8>,
}

/// Lo stemma decodificato una volta sola; `None` se il tema Bondeno non è quello in uso, così chi disegna
/// non deve controllare altro.
pub fn immagine(stemma: Stemma) -> Option<&'static Immagine> {
    if bondeno() { carica(stemma) } else { None }
}

/// Lo stemma decodificato, qualunque sia il tema.
pub fn carica(stemma: Stemma) -> Option<&'static Immagine> {
    use std::sync::OnceLock;
    static SOLO: OnceLock<Option<Immagine>> = OnceLock::new();
    static SCRITTA_: OnceLock<Option<Immagine>> = OnceLock::new();
    static CERCHIO_: OnceLock<Option<Immagine>> = OnceLock::new();
    let (cell, png, ritaglio) = match stemma {
        Stemma::Solo => (&SOLO, STEMMA, None),
        Stemma::Scritta => (&SCRITTA_, SCRITTA, Some(SCRITTA_RITAGLIO)),
        Stemma::Cerchio => (&CERCHIO_, CERCHIO, None),
    };
    cell.get_or_init(|| decodifica(png, ritaglio)).as_ref()
}

fn decodifica(png: &[u8], ritaglio: Option<(u32, u32, u32)>) -> Option<Immagine> {
    let mut rgba = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?.to_rgba8();
    if let Some((x, y, lato)) = ritaglio {
        rgba = image::imageops::crop_imm(&rgba, x, y, lato, lato).to_image();
    }
    let (w, h) = rgba.dimensions();
    let mut bgra = rgba.into_raw();
    for px in bgra.chunks_exact_mut(4) {
        let a = px[3] as u32;
        let (r, g, b) = (px[0] as u32, px[1] as u32, px[2] as u32);
        px[0] = ((b * a + 127) / 255) as u8;
        px[1] = ((g * a + 127) / 255) as u8;
        px[2] = ((r * a + 127) / 255) as u8;
    }
    Some(Immagine { w, h, bgra })
}

/// «Cedro 0.21.0», più il nome dell'edizione con il tema Bondeno.
pub fn titolo(versione: &str) -> String {
    titolo_con(versione, nome())
}

fn titolo_con(versione: &str, nome: Option<&str>) -> String {
    match nome {
        Some(nome) => format!("Cedro {versione} · {nome}"),
        None => format!("Cedro {versione}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn il_tema_si_riconosce_dalla_chiave() {
        assert!(e_bondeno("bondeno") && e_bondeno(" bondeno\n"));
        assert!(!e_bondeno("banco-di-lavoro") && !e_bondeno(""));
        assert_eq!(titolo_con("1.2.3", Some(NOME)), "Cedro 1.2.3 · Comune di Bondeno");
        assert_eq!(titolo_con("1.2.3", None), "Cedro 1.2.3");
    }

    #[test]
    fn gli_stemmi_si_decodificano() {
        for (stemma, lato) in [(Stemma::Solo, 300), (Stemma::Scritta, 200), (Stemma::Cerchio, 300)] {
            let img = carica(stemma).unwrap();
            assert_eq!((img.w, img.h, img.bgra.len()), (lato, lato, (lato * lato * 4) as usize), "{stemma:?}");
            // Premoltiplicato: nessun canale supera l'opacità.
            assert!(img.bgra.chunks_exact(4).all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]));
        }
    }
}
