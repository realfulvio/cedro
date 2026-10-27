//! Le due edizioni di Cedro: quella standard e quella per il Comune di Bondeno (`--features bondeno`).
//! Motore e funzioni sono gli stessi; cambiano lo stemma, qualche scritta e i pacchetti da cui si aggiorna.
//! Lo stemma è l'originale del sito del Comune (assets/bondeno/FONTE.md): si mostra così com'è, mai ritoccato.

/// Vero nell'edizione per il Comune di Bondeno.
pub const BONDENO: bool = cfg!(feature = "bondeno");

/// Il nome dell'edizione da mostrare accanto alla versione; niente per quella standard.
pub const NOME: Option<&str> = if BONDENO { Some("Edizione Comune di Bondeno") } else { None };

/// L'installatore per Windows x64 di questa edizione: `<prefisso><versione>.exe`.
/// Il prefisso Bondeno non comincia con «Cedro-Setup»: le versioni standard (anche le vecchie) non lo prendono.
pub const INSTALLER_X64: &str = if BONDENO { "Cedro-Bondeno-Setup-" } else { "Cedro-Setup-" };
/// L'installatore per Windows on ARM di questa edizione.
pub const INSTALLER_ARM64: &str = if BONDENO { "Cedro-Bondeno-ARM64-Setup-" } else { "Cedro-ARM64-Setup-" };
/// Il pacchetto per Linux: `<prefisso><versione>-linux-<arch>.tar.gz`.
pub const PACCHETTO_LINUX: &str = if BONDENO { "cedro-bondeno-" } else { "cedro-" };

/// L'icona dell'app (menu, area di notifica, Informazioni su Linux): lo Spicchio nella standard, la ruota nell'edizione Bondeno.
/// Su Windows la icona sta nell'exe (build.rs sceglie `assets/cedro.ico` o `assets/cedro-bondeno.ico`).
#[cfg(feature = "bondeno")]
pub const ICONA: &[u8] = include_bytes!("../docs/mark.png");
#[cfg(not(feature = "bondeno"))]
pub const ICONA: &[u8] = include_bytes!("../assets/icon-256.png");

/// Gli stemmi stanno solo nell'edizione Bondeno; in quella standard sono vuoti.
///
/// Solo lo stemma, a colori, su fondo trasparente (PNG 300×300).
#[cfg(feature = "bondeno")]
pub const STEMMA: &[u8] = include_bytes!("../assets/bondeno/Stemma-Comune-di-Bondeno.png");
#[cfg(not(feature = "bondeno"))]
pub const STEMMA: &[u8] = &[];
/// Lo stemma con la scritta «Città di Bondeno» dentro il cerchio, su fondo trasparente.
/// Il PNG del sito ha 50 pixel di margine vuoto per lato: chi lo disegna ritaglia [`SCRITTA_RITAGLIO`].
#[cfg(feature = "bondeno")]
pub const SCRITTA: &[u8] = include_bytes!("../assets/bondeno/Stemma-del-Comune-di-Bondeno-1.png");
#[cfg(not(feature = "bondeno"))]
pub const SCRITTA: &[u8] = &[];
/// Il quadrato utile di [`SCRITTA`]: x, y, lato (il resto è trasparente).
pub const SCRITTA_RITAGLIO: (u32, u32, u32) = (50, 50, 200);
/// Lo stemma con scritta in un cerchio bianco pieno: si legge sulla barra blu.
#[cfg(feature = "bondeno")]
pub const CERCHIO: &[u8] = include_bytes!("../assets/bondeno/Stemma-del-Comune-di-Bondeno-3.png");
#[cfg(not(feature = "bondeno"))]
pub const CERCHIO: &[u8] = &[];

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

/// Lo stemma decodificato una volta sola; `None` nell'edizione standard.
pub fn immagine(stemma: Stemma) -> Option<&'static Immagine> {
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
    if png.is_empty() {
        return None;
    }
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

/// «Cedro 0.19.0», più l'edizione se non è quella standard.
pub fn titolo(versione: &str) -> String {
    match NOME {
        Some(nome) => format!("Cedro {versione} · {nome}"),
        None => format!("Cedro {versione}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn i_nomi_dei_pacchetti_non_si_confondono() {
        // Le regole delle versioni vecchie: x64 fino alla 0.18 = «Cedro-Setup*», ARM 0.18 = «Cedro-ARM64-Setup*».
        for p in ["Cedro-Bondeno-Setup-", "Cedro-Bondeno-ARM64-Setup-"] {
            assert!(!p.starts_with("Cedro-Setup") && !p.starts_with("Cedro-ARM64-Setup"));
        }
        // Il pacchetto Linux standard comincia con una cifra dopo «cedro-».
        assert!(!"cedro-bondeno-0.19.0-linux-x64.tar.gz"["cedro-".len()..].starts_with(|c: char| c.is_ascii_digit()));
    }

    #[test]
    #[cfg(feature = "bondeno")]
    fn gli_stemmi_si_decodificano() {
        for (stemma, lato) in [(Stemma::Solo, 300), (Stemma::Scritta, 200), (Stemma::Cerchio, 300)] {
            let img = immagine(stemma).unwrap();
            assert_eq!((img.w, img.h, img.bgra.len()), (lato, lato, (lato * lato * 4) as usize), "{stemma:?}");
            // Premoltiplicato: nessun canale supera l'opacità.
            assert!(img.bgra.chunks_exact(4).all(|p| p[0] <= p[3] && p[1] <= p[3] && p[2] <= p[3]));
        }
    }

    #[test]
    #[cfg(not(feature = "bondeno"))]
    fn la_standard_non_ha_stemmi() {
        assert!(immagine(Stemma::Solo).is_none() && NOME.is_none());
    }
}
