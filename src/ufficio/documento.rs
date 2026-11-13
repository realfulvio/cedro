//! Il documento di testo com'è nel file (Word, Writer, RTF): sezioni con la loro pagina, paragrafi
//! con stili diretti già risolti, tabelle, immagini, elenchi numerati, intestazioni e piè di pagina,
//! note. I lettori (docx.rs, odt.rs, rtf.rs, doc.rs) lo riempiono; `impagina_doc` lo mette in pagina.
//! Misure in punti.

use super::pagina::Rgb;

#[derive(Clone, Debug, PartialEq)]
pub struct Font {
    pub family: String,
    pub size: f32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub double_underline: bool,
    pub strike: bool,
    pub color: Rgb,
    pub highlight: Option<Rgb>,
    /// 1 apice, -1 pedice.
    pub vert: i8,
    pub caps: bool,
    pub small_caps: bool,
    /// Spaziatura in più tra i caratteri (punti).
    pub spacing: f32,
}

impl Default for Font {
    fn default() -> Self {
        Font {
            family: "Times New Roman".into(),
            size: 10.0,
            bold: false,
            italic: false,
            underline: false,
            double_underline: false,
            strike: false,
            color: (0, 0, 0),
            highlight: None,
            vert: 0,
            caps: false,
            small_caps: false,
            spacing: 0.0,
        }
    }
}

#[derive(Clone, Debug)]
pub enum Pezzo {
    Testo(String),
    Tab,
    /// A capo dentro il paragrafo.
    ACapo,
    /// Salto di pagina o di colonna dentro il paragrafo.
    SaltoPagina,
    SaltoColonna,
    /// Un'immagine nel testo, grande `w`×`h`.
    Immagine { img: usize, w: f32, h: f32 },
    /// Il richiamo di una nota a piè di pagina (indice in `Documento::note`) o di chiusura.
    Nota(usize),
    NotaFine(usize),
    /// Numero della pagina, totale delle pagine, pagine della sezione.
    NumPagina,
    NumPagine,
}

#[derive(Clone, Debug)]
pub struct Run {
    pub pezzo: Pezzo,
    pub font: Font,
    pub link: Option<super::pagina::Target>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Allinea {
    #[default]
    Sinistra,
    Centro,
    Destra,
    Giustifica,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Interlinea {
    /// Multiplo dell'interlinea singola del carattere.
    Multipla(f32),
    Esatta(f32),
    Minima(f32),
}

impl Default for Interlinea {
    fn default() -> Self {
        Interlinea::Multipla(1.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TipoTab {
    Sinistra,
    Centro,
    Destra,
    Decimale,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Tab {
    pub pos: f32,
    pub tipo: TipoTab,
    /// Il carattere di riempimento: '.', '-', '_' o nessuno.
    pub leader: Option<char>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Linea {
    pub width: f32,
    pub color: Rgb,
    pub dash: super::pagina::Dash,
    pub double: bool,
    /// Distanza dal testo.
    pub space: f32,
}

#[derive(Clone, Debug, Default)]
pub struct Paragrafo {
    pub runs: Vec<Run>,
    pub allinea: Allinea,
    pub rientro_sx: f32,
    pub rientro_dx: f32,
    /// Prima riga: positivo rientra, negativo sporge.
    pub prima_riga: f32,
    pub prima: f32,
    pub dopo: f32,
    pub interlinea: Interlinea,
    pub insieme_al_seguente: bool,
    pub righe_insieme: bool,
    pub salto_prima: bool,
    pub vedove: bool,
    pub tabs: Vec<Tab>,
    /// Il numero o il punto dell'elenco, già scritto, con il suo carattere.
    pub etichetta: Option<(String, Font)>,
    /// Dopo l'etichetta: tabulazione (true) o spazio.
    pub etichetta_tab: bool,
    /// Livello del titolo (1…9) per i segnalibri del PDF.
    pub titolo: Option<u8>,
    pub sfondo: Option<Rgb>,
    /// sopra, sotto, sinistra, destra, tra paragrafi con lo stesso bordo
    pub bordi: [Option<Linea>; 5],
    /// Il carattere del segno di paragrafo: dà l'altezza a un paragrafo vuoto.
    pub font_fine: Font,
    /// Lo spazio prima/dopo non c'è tra paragrafi dello stesso stile (spaziatura contestuale).
    pub contestuale: bool,
    pub stile: String,
    /// I segnalibri che cominciano qui (arrivo dei collegamenti interni).
    pub segnalibri: Vec<String>,
    /// Spaziatura automatica (come in HTML): non si somma con quella del vicino e sparisce ai bordi
    /// delle celle.
    pub auto_prima: bool,
    pub auto_dopo: bool,
    /// Interlinea come Writer: il multiplo si applica al testo, non alle immagini nella riga.
    pub interlinea_testo: bool,
}

#[derive(Clone, Debug)]
pub struct Cella {
    pub blocchi: Vec<Blocco>,
    /// Quante colonne della griglia occupa.
    pub span: usize,
    /// Unione verticale: `Some(true)` comincia, `Some(false)` continua quella sopra.
    pub vmerge: Option<bool>,
    pub sfondo: Option<Rgb>,
    /// sopra, sotto, sinistra, destra
    pub bordi: [Option<Linea>; 4],
    /// 0 in alto, 1 al centro, 2 in basso.
    pub valign: u8,
    /// sinistra, destra, sopra, sotto
    pub margini: [f32; 4],
    pub larghezza: Option<f32>,
}

#[derive(Clone, Debug)]
pub struct Riga {
    pub celle: Vec<Cella>,
    /// Altezza: positiva «almeno», negativa «esatta».
    pub altezza: f32,
    pub intestazione: bool,
    pub non_dividere: bool,
}

#[derive(Clone, Debug)]
pub struct Tabella {
    pub griglia: Vec<f32>,
    pub righe: Vec<Riga>,
    /// 0 sinistra, 1 centro, 2 destra.
    pub allinea: u8,
    pub rientro: f32,
    /// Bordi interni tra celle (orizzontali e verticali) se la cella non dice altro.
    pub bordo_h: Option<Linea>,
    pub bordo_v: Option<Linea>,
    /// Larghezza in parte dello spazio disponibile (ODT `rel-width`): la griglia è in proporzione.
    pub relativa: Option<f32>,
}

/// Un oggetto fuori dal testo (immagine o casella di testo ancorata): posizione sulla pagina.
#[derive(Clone, Debug)]
pub struct Mobile {
    pub contenuto: MobileContenuto,
    pub w: f32,
    pub h: f32,
    /// Orizzontale: dal bordo della pagina, dal margine o dalla colonna; `None` = allineato.
    pub x: Riferimento,
    pub y: Riferimento,
    /// Il testo gira attorno (true) o ci passa sopra/sotto (false).
    pub avvolge: Avvolge,
    pub dietro: bool,
}

#[derive(Clone, Debug)]
pub enum MobileContenuto {
    Immagine(usize),
    Testo(Vec<Blocco>, Option<Rgb>, Option<Linea>),
    /// Paragrafi in una cornice (framePr): senza margini interni; `w`/`h` a 0 = misura del testo.
    Cornice(Vec<Blocco>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Base {
    Pagina,
    Margine,
    Colonna,
    Paragrafo,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Riferimento {
    Spostato(Base, f32),
    /// 0 sinistra/sopra, 1 centro, 2 destra/sotto.
    Allineato(Base, u8),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Avvolge {
    /// Il testo passa ai lati (quadrato, stretto, attraverso).
    Lati,
    /// Il testo salta sopra e sotto.
    SopraSotto,
    /// Davanti o dietro al testo, senza spostarlo.
    Nessuno,
}

#[derive(Clone, Debug)]
pub enum Blocco {
    Par(Paragrafo),
    Tab(Tabella),
    /// Un oggetto ancorato al paragrafo che segue.
    Mobile(Mobile),
}

#[derive(Clone, Debug, Default)]
pub struct Intestazioni {
    pub normale: Option<Vec<Blocco>>,
    pub prima: Option<Vec<Blocco>>,
    pub pari: Option<Vec<Blocco>>,
}

#[derive(Clone, Debug)]
pub struct Sezione {
    pub w: f32,
    pub h: f32,
    /// sinistra, destra, sopra, sotto
    pub margini: [f32; 4],
    pub dist_testa: f32,
    pub dist_piede: f32,
    pub colonne: u8,
    pub spazio_colonne: f32,
    pub testa: Intestazioni,
    pub piede: Intestazioni,
    pub prima_diversa: bool,
    /// Comincia su una pagina nuova (altrimenti continua sulla stessa).
    pub pagina_nuova: bool,
    /// 1 dispari, 2 pari: la sezione comincia su una pagina dispari/pari.
    pub parita: u8,
    pub primo_numero: Option<u32>,
    pub blocchi: Vec<Blocco>,
}

impl Default for Sezione {
    fn default() -> Self {
        // A4 con i margini di Word (2,5 cm sopra e sotto, 2 cm ai lati).
        Sezione {
            w: 595.3,
            h: 841.9,
            margini: [56.7, 56.7, 70.85, 56.7],
            dist_testa: 35.4,
            dist_piede: 35.4,
            colonne: 1,
            spazio_colonne: 36.0,
            testa: Intestazioni::default(),
            piede: Intestazioni::default(),
            prima_diversa: false,
            pagina_nuova: true,
            parita: 0,
            primo_numero: None,
            blocchi: Vec::new(),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct Documento {
    pub sezioni: Vec<Sezione>,
    pub immagini: Vec<crate::pdfdoc::DocImage>,
    /// Le note a piè di pagina: il testo, già con il suo numero.
    pub note: Vec<(String, Vec<Blocco>)>,
    pub note_fine: Vec<(String, Vec<Blocco>)>,
    pub pari_dispari: bool,
    pub tab_predefinita: f32,
    pub titolo: String,
}

impl Paragrafo {
    pub fn testo(&self) -> String {
        let mut s = String::new();
        for r in &self.runs {
            match &r.pezzo {
                Pezzo::Testo(t) => s.push_str(t),
                Pezzo::Tab => s.push('\t'),
                Pezzo::ACapo => s.push('\n'),
                _ => {}
            }
        }
        s
    }
}
