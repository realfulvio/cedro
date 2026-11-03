//! La Guida di Cedro, nei colori e nel carattere del tema in uso: un indice a sinistra con otto
//! argomenti e a destra il testo con un riquadro animato che mostra la finestra vera. Si disegna con
//! `Painter` come tutto il resto dell'interfaccia, quindi è identica su Windows e Linux; le finestre
//! di sistema passano solo il tempo, il mouse e la tastiera.
//! Le coordinate sono in unità di disegno: la finestra è sempre `W` x `H`.

use super::assets as ic;
use super::core::{inside, R};
use super::gfx::{Align, Painter, Weight};
use super::screen::Key;
use super::theme::{mix, Palette, ThemeId};

pub const W: f32 = 1100.0;
pub const H: f32 = 680.0;

const HEAD_H: f32 = 56.0;
const FOOT_H: f32 = 56.0;
const NAV_W: f32 = 236.0;
const PAD: f32 = 28.0;
const COL_W: f32 = 340.0;
/// Quanto dura ogni argomento prima di passare al successivo.
const PAGE_MS: u64 = 9400;
/// Un «passo» dell'animazione: le cose compaiono una dopo l'altra a questo ritmo.
const STEP: f32 = 1100.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hit {
    Page(usize),
    Row(usize),
    Prev,
    Next,
}

/// Cosa deve fare la finestra dopo un evento.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Out {
    None,
    Redraw,
    Close,
}

struct Page {
    nav: &'static str,
    title: &'static str,
    intro: &'static str,
    rows: &'static [(&'static str, &'static str, &'static str)],
}

const PAGES: [Page; 8] = [
    Page {
        nav: "La ruota",
        title: "Tieni premuto un tasto, poi clic destro sul file",
        intro: "Si apre la ruota. Clic sulla fetta: la copia nasce accanto all'originale, che resta dov'è.",
        rows: &[
            ("Shift", "Converti", "Cambia formato: JPEG, PDF, MP4…"),
            ("Ctrl", "Modifica", "Comprimi, ruota, OCR, metadati; sui PDF anche Unisci e Dividi"),
            ("Alt", "Azioni sul file", "Duplica, copia il percorso, ZIP"),
        ],
    },
    Page {
        nav: "Conversioni",
        title: "Trascina i file, scegli, converti",
        intro: "Dal menu dell'icona apri Conversioni. Trascini file anche di tipi diversi: a sinistra le attività, al centro la coda, a destra le opzioni del file scelto.",
        rows: &[
            ("Coda", "Più file insieme", "Anche di tipi diversi, con stato e anteprima"),
            ("Opzioni", "Formato e qualità", "Dimensioni, raddrizza, PDF/A, bitrate, taglio"),
            ("Esiste", "Il file c'è già", "Cedro chiede se sostituire, tenere entrambi o saltare"),
        ],
    },
    Page {
        nav: "Strumenti PDF",
        title: "Unisci, dividi, riordina e comprimi i PDF",
        intro: "Nella scheda PDF ci sono quattro strumenti, oltre alla conversione. Gli originali non si toccano: il risultato è sempre una copia nuova.",
        rows: &[
            ("Unisci", "Più PDF in uno", "Trascina per l'ordine, scegli le pagine di ogni file"),
            ("Dividi", "Un PDF in più", "Per pagine o intervalli, con le miniature"),
            ("Pagine", "Ruota, togli, copia", "Clic sulle miniature, poi i pulsanti in alto"),
            ("Comprimi", "Peso ridotto", "Stima prima; mai più pesante dell'originale"),
        ],
    },
    Page {
        nav: "PDF in Word/Excel",
        title: "Da PDF a documento modificabile",
        intro: "Word mantiene colonne e impaginazione, Excel prende le tabelle. Funziona anche con i PDF scansionati: Cedro raddrizza la pagina e legge il testo in italiano.",
        rows: &[
            ("Word", "PDF in DOCX", "Colonne, titoli, immagini"),
            ("Excel", "PDF in XLSX", "Un foglio per tabella, numeri nelle celle giuste"),
            ("Scansione", "Anche le foto di pagine", "Raddrizza, OCR, riconosce le tabelle"),
        ],
    },
    Page {
        nav: "Trascrizione",
        title: "Dalla voce al testo, poi rivedilo",
        intro: "Trascrivi vocali, registrazioni e l'audio dei video; se c'è una scheda video compatibile Cedro la usa. Riconosce chi parla e in Rivedi riascolti ogni frase, dai i nomi e correggi i punti dubbi.",
        rows: &[
            ("Chi parla", "Le voci riconosciute", "Un nome e un colore per ogni interlocutore"),
            ("Rivedi", "Audio e testo insieme", "Play per frase, i punti da controllare in evidenza"),
            ("Locale", "Resta sul PC", "Il motore si scarica al primo uso"),
        ],
    },
    Page {
        nav: "YouTube e web",
        title: "Incolla un link, scegli Video o Solo audio",
        intro: "Anche più link in un colpo. Gli strumenti si scaricano da soli la prima volta e si tengono aggiornati.",
        rows: &[
            ("Link", "Anche elenchi", "Un link per riga"),
            ("Video", "MP4", "Risoluzione a scelta"),
            ("Audio", "MP3 o M4A", "Senza immagini"),
        ],
    },
    Page {
        nav: "Aspetto",
        title: "La finestra come la vuoi tu",
        intro: "In Impostazioni, Aspetto: sei temi, la modalità Standard (tutte le opzioni) o Semplificata (solo l'essenziale) e i caratteri più grandi. Ogni tema ha il suo carattere. Questa Guida cambia insieme a loro.",
        rows: &[
            ("Tema", "Sei disposizioni", "Dal Windows essenziale al Comune di Bondeno"),
            ("Modo", "Standard o Semplificata", "Il resto sta in «Altre opzioni»"),
            ("Testo", "Caratteri più grandi", "Più leggibili in tutta l'app"),
        ],
    },
    Page {
        nav: "Aggiornamenti",
        title: "Si tiene aggiornato da solo",
        intro: "Cedro controlla le novità e chiede prima di installare. Impostazioni, Sistema mostra processore, scheda video e memoria; Informazioni ha versione e licenze.",
        rows: &[
            ("x64", "Windows 10 e 11", "Il solito installatore"),
            ("ARM64", "Windows on ARM", "Versione nativa, non emulata"),
            ("Linux", "Wayland", "KDE Plasma e GNOME"),
        ],
    },
];

pub const PAGE_COUNT: usize = PAGES.len();

pub struct Guida {
    pub theme: ThemeId,
    pub page: usize,
    start: u64,
    hold: Option<usize>,
    hover: Option<Hit>,
    zones: Vec<(R, Hit)>,
}

impl Guida {
    pub fn new(theme: ThemeId, now: u64) -> Guida {
        Guida { theme, page: 0, start: now, hold: None, hover: None, zones: Vec::new() }
    }

    fn goto(&mut self, page: usize, now: u64) {
        self.page = page % PAGE_COUNT;
        self.start = now;
        self.hold = None;
    }

    /// Fa andare avanti da solo l'argomento scaduto; dopo l'ultimo si ricomincia.
    pub fn tick(&mut self, now: u64) {
        if now.saturating_sub(self.start) >= PAGE_MS && self.hold.is_none() {
            self.goto(self.page + 1, now);
        }
    }

    pub fn is_hand(&self) -> bool {
        self.hover.is_some()
    }

    fn hit_at(&self, x: f32, y: f32) -> Option<Hit> {
        self.zones.iter().rev().find(|(r, _)| inside(*r, x, y)).map(|(_, h)| *h)
    }

    pub fn pointer_move(&mut self, x: f32, y: f32) -> bool {
        let h = self.hit_at(x, y);
        let changed = h != self.hover;
        self.hover = h;
        changed
    }

    pub fn pointer_leave(&mut self) -> bool {
        self.hover.take().is_some()
    }

    pub fn pointer_down(&mut self, x: f32, y: f32, now: u64) -> Out {
        match self.hit_at(x, y) {
            Some(h) => self.apply(h, now),
            None => Out::None,
        }
    }

    fn apply(&mut self, hit: Hit, now: u64) -> Out {
        match hit {
            Hit::Page(i) => self.goto(i, now),
            Hit::Prev => self.goto(self.page + PAGE_COUNT - 1, now),
            Hit::Next if self.page + 1 == PAGE_COUNT => return Out::Close,
            Hit::Next => self.goto(self.page + 1, now),
            Hit::Row(i) => {
                // Un clic su una riga ferma l'animazione su quella; un secondo clic la lascia ripartire.
                self.hold = if self.hold == Some(i) { None } else { Some(i) };
                self.start = now;
            }
        }
        Out::Redraw
    }

    pub fn key(&mut self, key: Key, now: u64) -> Out {
        match key {
            Key::Escape => Out::Close,
            Key::Right | Key::Down | Key::Enter | Key::Space => self.apply(Hit::Next, now),
            Key::Left | Key::Up if self.page > 0 => self.apply(Hit::Prev, now),
            _ => Out::None,
        }
    }

    /// Disegna tutta la finestra (`W` x `H` unità) al momento `now`.
    pub fn draw(&mut self, p: &mut Painter, now: u64) {
        let pal = self.theme.palette();
        let t = now.saturating_sub(self.start) as f32;
        self.zones.clear();
        p.clear(pal.bg);
        self.head(p, pal);
        self.nav(p, pal);
        p.fill(NAV_W, HEAD_H, W - NAV_W, H - HEAD_H - FOOT_H, pal.work);
        let page = &PAGES[self.page];
        self.text_column(p, pal, page, t);
        let stage = (NAV_W + PAD + COL_W + 26.0, HEAD_H + 24.0, W - PAD - (NAV_W + PAD + COL_W + 26.0), H - HEAD_H - FOOT_H - 48.0);
        p.rrect(stage.0, stage.1, stage.2, stage.3, 12.0, Some(pal.surface), Some((pal.border, 1.0)));
        p.push_clip(stage.0, stage.1, stage.2, stage.3);
        let mut s = Stage { p, pal, x: stage.0, y: stage.1, w: stage.2, h: stage.3, t, lit: self.lit_row(t), hold: self.hold };
        match self.page {
            0 => s.ruota(),
            1 => s.conversioni(),
            2 => s.pdf(),
            3 => s.word_excel(),
            4 => s.trascrizione(),
            5 => s.youtube(),
            6 => s.aspetto(),
            _ => s.aggiornamenti(),
        }
        p.pop_clip();
        self.foot(p, pal);
    }

    /// La riga in evidenza: quella scelta con un clic, altrimenti segue l'animazione.
    fn lit_row(&self, t: f32) -> usize {
        let n = PAGES[self.page].rows.len();
        match self.hold {
            Some(i) => i.min(n - 1),
            None if self.page == 0 => ((t / MODE_MS) as usize) % 3,
            None => ((t / (PAGE_MS as f32 / n as f32)) as usize).min(n - 1),
        }
    }

    fn head(&self, p: &mut Painter, pal: &Palette) {
        p.fill(0.0, 0.0, W, HEAD_H, pal.surface);
        p.fill(0.0, HEAD_H - 1.0, W, 1.0, pal.border);
        let mut x = 20.0;
        if crate::edizione::bondeno() {
            p.stemma(crate::edizione::Stemma::Cerchio, x, 11.0, 34.0);
            x += 46.0;
        } else {
            p.symbol(x, 14.0, 28.0);
            x += 40.0;
        }
        let title = if crate::edizione::bondeno() { "Guida di Cedro · Comune di Bondeno" } else { "Guida di Cedro" };
        p.text(title, x, 0.0, 520.0, HEAD_H, 16.0, Weight::Bold, pal.text, Align::Left);
        p.text(&format!("{} di {}", self.page + 1, PAGE_COUNT), W - 140.0, 0.0, 120.0, HEAD_H, 12.0, Weight::Regular, pal.muted, Align::Right);
    }

    fn nav(&mut self, p: &mut Painter, pal: &Palette) {
        p.fill(NAV_W - 1.0, HEAD_H, 1.0, H - HEAD_H - FOOT_H, pal.border);
        for (i, page) in PAGES.iter().enumerate() {
            let r = (10.0, HEAD_H + 14.0 + i as f32 * 46.0, NAV_W - 22.0, 40.0);
            let on = i == self.page;
            let hot = self.hover == Some(Hit::Page(i));
            if on || hot {
                p.rrect(r.0, r.1, r.2, r.3, 8.0, Some(if on { pal.nav_sel } else { pal.alt }), None);
            }
            p.rrect(r.0 + 12.0, r.1 + 11.0, 18.0, 18.0, 5.0, Some(if on { pal.primary } else { pal.border }), None);
            p.text(page.nav, r.0 + 42.0, r.1, r.2 - 48.0, r.3, 13.0, if on { Weight::Bold } else { Weight::Regular }, pal.text, Align::Left);
            self.zones.push((r, Hit::Page(i)));
        }
    }

    fn text_column(&mut self, p: &mut Painter, pal: &Palette, page: &Page, _t: f32) {
        let x = NAV_W + PAD;
        let mut y = HEAD_H + 24.0;
        y += p.text_wrap(page.title, x, y, COL_W, 22.0, Weight::Bold, pal.text, 28.0, 3) + 8.0;
        y += p.text_wrap(page.intro, x, y, COL_W, 13.0, Weight::Regular, pal.muted, 20.0, 7) + 14.0;
        let lit = self.lit_row(_t);
        for (i, (key, name, detail)) in page.rows.iter().enumerate() {
            let r = (x, y, COL_W, 72.0);
            let on = i == lit;
            let hot = self.hover == Some(Hit::Row(i));
            p.rrect(r.0, r.1, r.2, r.3, 10.0, Some(if hot && !on { pal.alt } else { pal.surface }), Some((if on { pal.primary } else { pal.border }, if on { 2.0 } else { 1.0 })));
            let kw = 74.0;
            p.rrect(r.0 + 10.0, r.1 + 21.0, kw, 30.0, 7.0, Some(if on { pal.primary } else { pal.alt }), None);
            p.text(key, r.0 + 10.0, r.1 + 21.0, kw, 30.0, 12.0, Weight::Bold, if on { pal.on_primary } else { pal.text }, Align::Center);
            p.text(name, r.0 + 96.0, r.1 + 5.0, r.2 - 106.0, 24.0, 13.0, Weight::Bold, pal.text, Align::Left);
            p.text_wrap(detail, r.0 + 96.0, r.1 + 29.0, r.2 - 106.0, 11.0, Weight::Regular, pal.muted, 15.0, 2);
            self.zones.push((r, Hit::Row(i)));
            y += 80.0;
        }
    }

    fn foot(&mut self, p: &mut Painter, pal: &Palette) {
        let y0 = H - FOOT_H;
        p.fill(0.0, y0, W, FOOT_H, pal.surface);
        p.fill(0.0, y0, W, 1.0, pal.border);
        let mut x = 20.0;
        for i in 0..PAGE_COUNT {
            let w = if i == self.page { 26.0 } else { 10.0 };
            p.rrect(x, y0 + 23.0, w, 10.0, 5.0, Some(if i == self.page { pal.primary } else { pal.border }), None);
            x += w + 8.0;
        }
        let sign = if crate::edizione::bondeno() {
            format!("Cedro {} · Comune di Bondeno · Powered by Luca real>hcok_ A.", crate::update::CURRENT)
        } else {
            format!("Cedro {} · Powered by Luca real>hcok_ A.", crate::update::CURRENT)
        };
        p.text(&sign, 270.0, y0, 440.0, FOOT_H, 11.0, Weight::Regular, pal.muted, Align::Left);
        let (bw, bh) = (104.0, 36.0);
        let next = (W - 20.0 - bw, y0 + 10.0, bw, bh);
        let prev = (next.0 - 10.0 - bw, y0 + 10.0, bw, bh);
        if self.page > 0 {
            let hot = self.hover == Some(Hit::Prev);
            p.rrect(prev.0, prev.1, prev.2, prev.3, 8.0, Some(if hot { pal.alt } else { pal.surface }), Some((pal.border, 1.0)));
            p.text("Indietro", prev.0, prev.1, prev.2, prev.3, 13.0, Weight::Bold, pal.text, Align::Center);
            self.zones.push((prev, Hit::Prev));
        }
        let hot = self.hover == Some(Hit::Next);
        let fill = if hot { mix(pal.primary, pal.text, 0.22) } else { pal.primary };
        p.rrect(next.0, next.1, next.2, next.3, 8.0, Some(fill), None);
        let label = if self.page + 1 == PAGE_COUNT { "Chiudi" } else { "Avanti" };
        p.text(label, next.0, next.1, next.2, next.3, 13.0, Weight::Bold, pal.on_primary, Align::Center);
        self.zones.push((next, Hit::Next));
    }
}

/// Quanto dura ogni modo della ruota nel primo argomento.
const MODE_MS: f32 = PAGE_MS as f32 / 3.0;

fn ease(x: f32) -> f32 {
    let x = x.clamp(0.0, 1.0);
    x * x * (3.0 - 2.0 * x)
}

/// Il riquadro animato: coordinate relative al suo angolo alto a sinistra.
struct Stage<'a, 'b> {
    p: &'a mut Painter<'b>,
    pal: &'static Palette,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    t: f32,
    lit: usize,
    hold: Option<usize>,
}

impl Stage<'_, '_> {
    /// 0 prima di `at_step`, poi sale a 1 in mezzo secondo.
    fn show(&self, at_step: f32) -> f32 {
        ease((self.t - at_step * STEP) / 480.0)
    }

    /// Il colore `c` come se fosse sul fondo del riquadro con opacità `a`.
    fn fade(&self, c: (u8, u8, u8), a: f32) -> (u8, u8, u8) {
        mix(self.pal.surface, c, a)
    }

    fn chip(&mut self, x: f32, y: f32, label: &str, on: bool, a: f32) -> f32 {
        let pal = self.pal;
        let tw = self.p.measure(label, 12.0, Weight::Semibold);
        let w = tw + 26.0;
        let (fill, line, ink) = if on { (pal.primary, pal.primary, pal.on_primary) } else { (pal.surface, pal.border, pal.text) };
        self.p.rrect(self.x + x, self.y + y, w, 32.0, 7.0, Some(self.fade(fill, a)), Some((self.fade(line, a), 1.0)));
        self.p.text(label, self.x + x, self.y + y, w, 32.0, 12.0, Weight::Semibold, self.fade(ink, a), Align::Center);
        w
    }

    fn chips(&mut self, y: f32, items: &[(&str, bool)], at_step: f32) {
        let mut x = 18.0;
        for (i, (label, on)) in items.iter().enumerate() {
            let a = self.show(at_step + i as f32 * 0.35);
            let w = self.chip(x, y, label, *on, a);
            x += w + 8.0;
        }
    }

    /// Una riga di file: sigla, nome e, a destra, uno stato o una barra che avanza.
    fn file(&mut self, y: f32, tag: &str, name: &str, right: Right, a: f32) {
        let pal = self.pal;
        let (x, w) = (self.x + 18.0, self.w - 36.0);
        let y = self.y + y;
        self.p.rrect(x, y, w, 52.0, 9.0, Some(self.fade(pal.surface, a)), Some((self.fade(pal.border, a), 1.0)));
        self.p.rrect(x + 10.0, y + 9.0, 32.0, 34.0, 4.0, Some(self.fade(pal.alt, a)), None);
        self.p.text(tag, x + 10.0, y + 9.0, 32.0, 34.0, 9.0, Weight::Bold, self.fade(pal.primary, a), Align::Center);
        self.p.text(name, x + 54.0, y, w - 200.0, 52.0, 12.0, Weight::Semibold, self.fade(pal.text, a), Align::Left);
        match right {
            Right::Text(s, ok) => {
                let c = if ok { pal.ok } else { pal.muted };
                self.p.text(s, x + w - 190.0, y, 180.0, 52.0, 11.0, if ok { Weight::Bold } else { Weight::Regular }, self.fade(c, a), Align::Right);
            }
            Right::Bar(label) => {
                let (bx, bw) = (x + w - 190.0, 120.0);
                self.p.rrect(bx, y + 23.0, bw, 6.0, 3.0, Some(self.fade(pal.alt, a)), None);
                let k = ((self.t % 2600.0) / 2600.0).clamp(0.04, 1.0);
                self.p.rrect(bx, y + 23.0, bw * k, 6.0, 3.0, Some(self.fade(pal.primary, a)), None);
                self.p.text(label, bx + bw + 6.0, y, 60.0, 52.0, 11.0, Weight::Regular, self.fade(pal.muted, a), Align::Right);
            }
        }
    }

    fn caption(&mut self, s: &str) {
        let (x, y, w, c) = (self.x + 18.0, self.y + self.h - 40.0, self.w - 36.0, self.pal.muted);
        self.p.text_wrap(s, x, y, w, 11.0, Weight::Regular, c, 15.0, 2);
    }

    // ---------- le scene ----------

    fn ruota(&mut self) {
        let pal = self.pal;
        let mode = self.lit;
        let slices: [&[&str]; 3] = [
            &["JPEG", "WEBP", "PNG", "PDF", "GIF", "BMP", "TIFF"],
            &["Comprimi", "50%", "Bianco e nero", "Ruota", "OCR", "Metadati"],
            &["Duplica", "Percorso", "ZIP", "Maiuscole"],
        ];
        let list = slices[mode];
        let n = list.len();
        let into = if self.hold.is_some() { self.t } else { self.t - mode as f32 * MODE_MS };
        let active = ((into / 650.0) as usize) % n;
        let (cx, cy, ro, ri) = (self.x + self.w / 2.0, self.y + 215.0, 160.0, 62.0);
        super::wheel::disc_shapes(self.p, cx, cy, ri, ro, ro / 230.0, n, Some(active));
        let sweep = std::f32::consts::TAU / n as f32;
        for (i, label) in list.iter().enumerate() {
            let a = -std::f32::consts::FRAC_PI_2 + (i as f32 + 0.5) * sweep;
            let r = (ri + ro) / 2.0 + if i == active { 6.0 } else { 0.0 };
            let (lx, ly) = (cx + r * a.cos(), cy + r * a.sin());
            let ink = if i == active { (255, 255, 255) } else { (23, 43, 58) };
            self.p.text(label, lx - 46.0, ly - 12.0, 92.0, 24.0, 10.0, Weight::Semibold, ink, Align::Center);
        }
        // Il buco al centro: il file su cui si è fatto clic.
        self.p.circle(cx, cy, ri - 14.0, Some(pal.alt), Some((pal.border, 1.0)));
        self.p.icon(ic::IMAGE, cx - 14.0, cy - 22.0, 28.0, pal.primary);
        self.p.text("foto.jpg", cx - 44.0, cy + 8.0, 88.0, 20.0, 9.0, Weight::Regular, pal.muted, Align::Center);
        let keys = [("Shift", "Converti"), ("Ctrl", "Modifica"), ("Alt", "Azioni")];
        let mut x = self.w / 2.0 - 175.0;
        for (i, (k, name)) in keys.iter().enumerate() {
            let label = format!("{k} · {name}");
            let w = self.chip(x, 400.0, &label, i == mode, 1.0);
            x += w + 8.0;
        }
        self.caption("La ruota è uguale in tutti i temi. Clic su una riga a sinistra per fermarti su quel modo.");
    }

    fn conversioni(&mut self) {
        self.file(24.0, "JPG", "vacanza-01.heic", Right::Text("Fatto", true), self.show(0.4));
        self.file(88.0, "PDF", "delibera.pdf", Right::Bar("in corso"), self.show(1.4));
        self.file(152.0, "MP4", "riunione.mov", Right::Text("In coda", false), self.show(2.4));
        self.chips(240.0, &[("Converti", true), ("Formato: JPEG", false), ("Qualità 85", false)], 3.3);
        let a = self.show(4.6);
        let pal = self.pal;
        let (x, y, w) = (self.x + 18.0, self.y + 296.0, self.w - 36.0);
        self.p.rrect(x, y, w, 96.0, 9.0, Some(self.fade(pal.alt, a)), None);
        self.p.text("Il file c'è già: sostituire, tenere entrambi o saltare?", x + 14.0, y + 6.0, w - 28.0, 36.0, 12.0, Weight::Semibold, self.fade(pal.text, a), Align::Left);
        let mut bx = x + 14.0;
        for (i, label) in ["Sostituisci", "Tieni entrambi", "Salta"].iter().enumerate() {
            let on = i == 1;
            let tw = self.p.measure(label, 11.0, Weight::Semibold) + 24.0;
            self.p.rrect(bx, y + 48.0, tw, 32.0, 7.0, Some(self.fade(if on { pal.primary } else { pal.surface }, a)), Some((self.fade(if on { pal.primary } else { pal.border }, a), 1.0)));
            self.p.text(label, bx, y + 48.0, tw, 32.0, 11.0, Weight::Semibold, self.fade(if on { pal.on_primary } else { pal.text }, a), Align::Center);
            bx += tw + 8.0;
        }
    }

    fn pdf(&mut self) {
        let pal = self.pal;
        // Otto miniature di pagina; la 2, la 3 e la 6 si scelgono una dopo l'altra.
        let (tw, th, gap) = (84.0, 112.0, 14.0);
        let x0 = self.x + (self.w - (4.0 * tw + 3.0 * gap)) / 2.0;
        let picks = [(2usize, 1.0f32), (3, 1.9), (6, 2.8)];
        for i in 0..8usize {
            let (col, row) = (i % 4, i / 4);
            let (px, py) = (x0 + col as f32 * (tw + gap), self.y + 28.0 + row as f32 * (th + 20.0));
            let pick = picks.iter().find(|(n, _)| *n == i + 1).map(|(_, s)| self.show(*s)).unwrap_or(0.0);
            self.p.rrect(px, py, tw, th, 4.0, Some(pal.work), Some((mix(pal.border, pal.primary, pick), 1.0 + pick * 1.6)));
            for l in 0..6 {
                self.p.rrect(px + 10.0, py + 14.0 + l as f32 * 12.0, tw - 20.0 - (l % 3) as f32 * 12.0, 4.0, 2.0, Some(pal.border), None);
            }
            self.p.text(&format!("{}", i + 1), px, py + th - 24.0, tw, 20.0, 10.0, Weight::Semibold, mix(pal.muted, pal.primary, pick), Align::Center);
        }
        self.chips(298.0, &[("Ruota 90°", false), ("Togli", false), ("Copia in un PDF nuovo", true)], 3.4);
        let a = self.show(5.2);
        self.file(350.0, "PDF", "bilancio.pdf", Right::Text("4,2 MB → 1,1 MB · −74%", true), a);
    }

    fn word_excel(&mut self) {
        let pal = self.pal;
        self.file(24.0, "SCAN", "delibera-scansione.pdf", Right::Text("storta e sfocata", false), self.show(0.4));
        let a = self.show(1.6);
        self.file(88.0, "OCR", "Leggo il testo…", Right::Bar(""), a);
        let rows = [["Voce", "2025", "2026"], ["Entrate", "412.300", "438.900"], ["Uscite", "398.100", "421.700"], ["Avanzo", "14.200", "17.200"]];
        let (tx, ty, cw, rh) = (self.x + 18.0, self.y + 176.0, (self.w - 36.0) / 3.0, 40.0);
        for (r, cells) in rows.iter().enumerate() {
            let a = self.show(3.0 + r as f32 * 0.6);
            for (c, cell) in cells.iter().enumerate() {
                let (cx, cy) = (tx + c as f32 * cw, ty + r as f32 * rh);
                let fill = if r == 0 { pal.alt } else { pal.surface };
                self.p.rrect(cx, cy, cw, rh, 0.0, Some(self.fade(fill, a)), Some((self.fade(pal.border, a), 1.0)));
                let align = if c == 0 { Align::Left } else { Align::Right };
                self.p.text(cell, cx + 10.0, cy, cw - 20.0, rh, 12.0, if r == 0 { Weight::Bold } else { Weight::Regular }, self.fade(pal.text, a), align);
            }
        }
        self.chips(360.0, &[("Salva come Excel", true), ("Salva come Word", false)], 6.0);
        self.caption("Esempio: la tabella ricavata da un PDF scansionato.");
    }

    fn trascrizione(&mut self) {
        let pal = self.pal;
        let lines = [("00:12", "Il consiglio comunale è convocato per", false), ("00:19", "giovedì alle diciotto", true), ("00:26", "presso la sala consiliare.", false)];
        for (i, (time, text, doubt)) in lines.iter().enumerate() {
            let a = self.show(0.5 + i as f32 * 0.9);
            let y = self.y + 28.0 + i as f32 * 78.0;
            let x = self.x + 18.0;
            let w = self.w - 36.0;
            let edge = if *doubt { (209, 155, 0) } else { pal.primary };
            self.p.rrect(x, y, w, 64.0, 8.0, Some(self.fade(pal.alt, a)), None);
            self.p.rrect(x, y, 4.0, 64.0, 2.0, Some(self.fade(edge, a)), None);
            self.p.text(time, x + 16.0, y, 54.0, 64.0, 11.0, Weight::Semibold, self.fade(pal.muted, a), Align::Left);
            self.p.text(text, x + 76.0, y, w - 90.0, if *doubt { 40.0 } else { 64.0 }, 12.0, Weight::Regular, self.fade(pal.text, a), Align::Left);
            if *doubt {
                self.p.text("da controllare", x + 76.0, y + 34.0, w - 90.0, 22.0, 10.0, Weight::Semibold, self.fade((209, 155, 0), a), Align::Left);
            }
        }
        self.chips(272.0, &[("▶ Riascolta", true), ("Conferma", false), ("Copia testo", false)], 3.6);
        // L'indicatore di ascolto che scorre sotto le frasi.
        let a = self.show(4.6);
        let (x, y, w) = (self.x + 18.0, self.y + 330.0, self.w - 36.0);
        self.p.rrect(x, y, w, 6.0, 3.0, Some(self.fade(pal.alt, a)), None);
        let k = ((self.t - 4.6 * STEP) / 3000.0).clamp(0.0, 1.0);
        self.p.rrect(x, y, w * k, 6.0, 3.0, Some(self.fade(pal.primary, a)), None);
        self.p.circle(x + w * k, y + 3.0, 7.0, Some(self.fade(pal.primary, a)), None);
        self.caption("Il motore gira sul tuo PC e usa la scheda video se può.");
    }

    fn youtube(&mut self) {
        let pal = self.pal;
        let (x, y, w) = (self.x + 18.0, self.y + 28.0, self.w - 36.0);
        self.p.rrect(x, y, w, 46.0, 8.0, Some(pal.work), Some((pal.border, 1.0)));
        let url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
        let shown = ((self.t - 300.0) / 55.0).clamp(0.0, url.len() as f32) as usize;
        let typed = &url[..shown];
        self.p.text(typed, x + 14.0, y, w - 28.0, 46.0, 12.0, Weight::Regular, pal.text, Align::Left);
        if shown < url.len() || (self.t / 500.0) as usize % 2 == 0 {
            let tw = self.p.measure(typed, 12.0, Weight::Regular);
            self.p.fill(x + 14.0 + tw + 2.0, y + 13.0, 2.0, 20.0, pal.text);
        }
        self.chips(98.0, &[("Video", true), ("Solo audio", false), ("MP4 · 1080p", false)], 3.2);
        self.file(158.0, "MP4", "Consiglio comunale", Right::Bar(""), self.show(4.4));
        self.file(222.0, "MP3", "Intervista sindaco", Right::Text("Fatto", true), self.show(5.4));
        self.caption("Gli strumenti si scaricano al primo uso e si aggiornano da soli.");
    }

    fn aspetto(&mut self) {
        let pal = self.pal;
        let names = [
            ("Windows essenziale", (0, 94, 142)),
            ("Percorso guidato", (237, 98, 12)),
            ("Cedro compatto", (112, 237, 180)),
            ("Banco di lavoro", (0, 184, 230)),
            ("Documento al centro", (102, 48, 121)),
            ("Comune di Bondeno", (0, 94, 142)),
        ];
        let at = ((self.t / 900.0) as usize) % 6;
        let (cw, ch, gap) = ((self.w - 36.0 - 20.0) / 3.0, 100.0, 10.0);
        for (i, (name, col)) in names.iter().enumerate() {
            let (cx, cy) = (self.x + 18.0 + (i % 3) as f32 * (cw + gap), self.y + 24.0 + (i / 3) as f32 * (ch + gap));
            let on = i == at;
            self.p.rrect(cx, cy, cw, ch, 9.0, Some(pal.surface), Some((if on { pal.primary } else { pal.border }, if on { 2.0 } else { 1.0 })));
            self.p.rrect(cx + 10.0, cy + 10.0, cw - 20.0, 34.0, 5.0, Some(*col), None);
            self.p.text_wrap(name, cx + 8.0, cy + 48.0, cw - 16.0, 10.0, Weight::Semibold, pal.text, 14.0, 2);
        }
        let on_std = ((self.t / 1800.0) as usize) % 2 == 0;
        self.chips(252.0, &[("Standard", on_std), ("Semplificata", !on_std)], 0.2);
        // Il testo normale e quello più grande.
        let (x, y, w) = (self.x + 18.0, self.y + 306.0, self.w - 36.0);
        self.p.rrect(x, y, w, 100.0, 9.0, Some(pal.alt), None);
        self.p.text("Caratteri più grandi", x + 14.0, y + 8.0, w - 28.0, 28.0, 12.0, Weight::Bold, pal.text, Align::Left);
        self.p.text("Prima: il testo era piccolo da leggere.", x + 14.0, y + 38.0, w - 28.0, 22.0, 9.0, Weight::Regular, pal.muted, Align::Left);
        self.p.text("Adesso: due punti in più, ovunque.", x + 14.0, y + 62.0, w - 28.0, 28.0, 11.0, Weight::Semibold, pal.text, Align::Left);
        self.caption("Il carattere cambia con il tema; la Guida segue il tema scelto.");
    }

    fn aggiornamenti(&mut self) {
        let a = self.show(0.4);
        self.file(24.0, "NEW", "È uscita una nuova versione di Cedro", Right::Text("Installa", true), a);
        self.file(88.0, "PC", "Ryzen 7 · Radeon · 16 GB", Right::Text("scheda video usata", true), self.show(1.6));
        self.file(152.0, "INFO", "Versione e licenze dei componenti", Right::Text("Informazioni", false), self.show(2.6));
        let a = self.show(3.8);
        let _ = a;
        self.chips(240.0, &[("Windows x64", false), ("Windows ARM64", false), ("Linux", false)], 3.8);
        self.caption("Stesse funzioni dappertutto: l'installatore giusto lo sceglie Cedro quando si aggiorna.");
    }
}

enum Right {
    Text(&'static str, bool),
    Bar(&'static str),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::gfx::{Fonts, GlyphCache};
    use crate::ui::theme::THEMES;

    #[test]
    fn ogni_tema_e_argomento_si_disegna_e_i_clic_funzionano() {
        let mut cache = GlyphCache::new();
        for theme in THEMES {
            let mut g = Guida::new(theme, 0);
            for page in 0..PAGE_COUNT {
                for now in [0u64, 3300, 7700] {
                    let mut p = Painter::new(W as u32 / 2, H as u32 / 2, 0.5, Fonts::for_theme(theme), &mut cache);
                    g.draw(&mut p, now);
                }
                assert_eq!(g.page, page);
                // «Avanti» sull'ultimo argomento chiude, altrimenti passa al successivo.
                let out = g.key(Key::Right, 0);
                assert_eq!(out == Out::Close, page + 1 == PAGE_COUNT);
            }
        }
    }

    #[test]
    fn un_clic_su_una_riga_ferma_l_animazione() {
        let mut g = Guida::new(ThemeId::WindowsEssenziale, 0);
        let mut cache = GlyphCache::new();
        let mut p = Painter::new(W as u32, H as u32, 1.0, Fonts::for_theme(g.theme), &mut cache);
        g.draw(&mut p, 0);
        let row = g.zones.iter().find(|(_, h)| *h == Hit::Row(2)).unwrap().0;
        assert_eq!(g.pointer_down(row.0 + 5.0, row.1 + 5.0, 100), Out::Redraw);
        g.tick(100 + PAGE_MS * 3);
        assert_eq!(g.page, 0, "ferma su una riga: non cambia argomento da solo");
        g.pointer_down(row.0 + 5.0, row.1 + 5.0, 200);
        g.tick(200 + PAGE_MS);
        assert_eq!(g.page, 1);
    }
}
