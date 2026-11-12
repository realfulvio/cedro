//! «Altre opzioni» (i formati e le opzioni del pannello Conversioni del computer, dalle stesse definizioni di
//! `cedro::conversioni`) e «Impostazioni» (nome del file a modello, se esiste già, qualità, DPI, lingua e pagine
//! dell'OCR). Ogni riga cambia davvero le `Options` del motore o le sue impostazioni.

use cedro::conversioni::{self, Control, OptionDef, Values};
use cedro::ui::gfx::{Align, Painter, Weight};
use cedro::ui::theme::mix;
use cedro::{Exists, Kind, Slice};

use super::{ic, App, Azione, Effetto, BARRA, TOCCO};
use super::pdf::Campo;

/// I formati arrivati con questa versione: la scelta lo dice.
const NUOVI: [&str; 11] = ["flac", "opus", "aiff", "avif", "jxl", "ico", "tga", "qoi", "pnm", "hdr", "exr"];

pub fn nuovo(id: &str) -> bool {
    NUOVI.contains(&id)
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum OpzAz {
    Indietro,
    Vai,
    Formato(usize),
    /// Un'opzione (indice nell'elenco di ora) e il valore nuovo.
    Valore(usize, u32),
    /// Più o meno su un'opzione a intervallo.
    Passo(usize, i32),
    // Impostazioni
    ApriImpostazioni,
    Gettone(u8),
    Esiste(u8),
    Qualita(i32),
    Dpi(u32),
    LinguaOcr(u8),
    PagineOcr(i32),
}

pub struct Opzioni {
    pub formato: usize,
    pub valori: Values,
    /// Aperte dalla finestra principale per questa categoria: «Fatto» torna lì con le scelte.
    pub da_casa: Option<Kind>,
}

/// I formati di un tipo di file: quelli del pannello Conversioni del computer.
pub fn formati(kind: Kind) -> Vec<Slice> {
    let s = cedro::panel_slices(kind);
    s.iter().copied().filter(|s| s.id != "none").collect()
}

pub const MODELLI: [&str; 4] = ["%nome%", "%data%", "%ora%", "%ext%"];
const DPI: [u32; 4] = [100, 144, 200, 300];
const LINGUE_OCR: [(&str, &str); 3] = [("auto", "Automatica"), ("it", "Italiano"), ("en", "English")];

impl App {
    /// Apre «Altre opzioni» sul formato `id` (o il primo).
    pub(super) fn apri_opzioni(&mut self, id: Option<&str>) -> Effetto {
        let f = formati(self.kind);
        if f.is_empty() {
            return Effetto::Niente;
        }
        let formato = id.and_then(|id| f.iter().position(|s| s.id == id || (id == "jpg" && s.id == "jpeg"))).unwrap_or(0);
        let valori = self.opzioni_salvate.take().unwrap_or_default();
        self.opzioni = Some(Opzioni { formato, valori, da_casa: None });
        self.scorri = 0.0;
        self.campo = None;
        Effetto::Ridisegna
    }

    /// Il tipo di file delle opzioni aperte (quello del file, o la categoria della finestra principale).
    fn kind_opz(&self) -> Kind {
        self.opzioni.as_ref().and_then(|o| o.da_casa).unwrap_or(self.kind)
    }

    fn defs(&self) -> Vec<&'static OptionDef> {
        let Some(o) = &self.opzioni else { return Vec::new() };
        let kind = self.kind_opz();
        let f = formati(kind);
        let id = f.get(o.formato).map(|s| s.id).unwrap_or("none");
        // Il nome a modello sta nelle Impostazioni: qui non si ripete.
        conversioni::option_defs(kind, id).into_iter().filter(|d| d.opt != conversioni::Opt::NameTemplate && conversioni::shown(d, &o.valori)).collect()
    }

    /// Il testo di un campo di «Altre opzioni» o delle Impostazioni.
    pub(super) fn testo_opzione(&mut self, c: Campo) -> Option<&mut String> {
        match c {
            Campo::Modello => Some(&mut self.modello_nome),
            _ => None,
        }
    }

    /// Un tasto su un campo di «Altre opzioni»: i testi stanno in `Values`.
    pub(super) fn tasto_opzione(&mut self, k: usize, t: super::Tasto) -> bool {
        let defs = self.defs();
        let (Some(def), Some(o)) = (defs.get(k).copied(), self.opzioni.as_mut()) else { return false };
        let mut s = o.valori.text(def).to_string();
        match t {
            super::Tasto::Canc => {
                s.pop();
            }
            super::Tasto::Car(c) if !c.is_control() && s.chars().count() < 120 => s.push(c),
            _ => return false,
        }
        o.valori.set_text(def, &s);
        true
    }

    pub(super) fn azione_opzioni(&mut self, az: OpzAz) -> Effetto {
        match az {
            OpzAz::Indietro => {
                if self.impostazioni {
                    self.impostazioni = false;
                } else if let Some(o) = self.opzioni.take() {
                    match o.da_casa {
                        Some(k) => self.da_opzioni(k, o.formato, o.valori),
                        None => self.opzioni_salvate = Some(o.valori),
                    }
                }
                self.scorri = 0.0;
                return Effetto::Ridisegna;
            }
            OpzAz::Vai => {
                let kind = self.kind_opz();
                if self.opzioni.as_ref().is_some_and(|o| o.da_casa.is_some()) {
                    if let Some(Opzioni { formato, valori, da_casa: Some(k) }) = self.opzioni.take() {
                        self.da_opzioni(k, formato, valori);
                    }
                    return Effetto::Ridisegna;
                }
                let defs_ok = {
                    let Some(o) = &self.opzioni else { return Effetto::Niente };
                    let f = formati(kind);
                    let id = f.get(o.formato).map(|s| s.id).unwrap_or("none");
                    conversioni::validate(&o.valori, kind, id).is_ok()
                };
                if !defs_ok {
                    return Effetto::Niente;
                }
                let Some(o) = self.opzioni.take() else { return Effetto::Niente };
                let f = formati(kind);
                let Some(s) = f.get(o.formato).copied() else { return Effetto::Niente };
                let opts = conversioni::options_from(&o.valori);
                self.opzioni_salvate = Some(o.valori);
                return self.avvia_con(s.id, s.label, true, Some(Box::new(opts)), Vec::new());
            }
            OpzAz::Formato(i) => {
                if let Some(o) = self.opzioni.as_mut() {
                    o.formato = i;
                }
            }
            OpzAz::Valore(k, v) => {
                let defs = self.defs();
                if let (Some(def), Some(o)) = (defs.get(k).copied(), self.opzioni.as_mut()) {
                    o.valori.set(def, v);
                }
            }
            OpzAz::Passo(k, d) => {
                let defs = self.defs();
                if let (Some(def), Some(o)) = (defs.get(k).copied(), self.opzioni.as_mut()) {
                    if let Control::Range { min, max, .. } = def.control {
                        let v = (o.valori.get(def) as i64 + d as i64).clamp(min as i64, max as i64) as u32;
                        o.valori.set(def, v);
                    }
                }
            }
            OpzAz::ApriImpostazioni => {
                self.impostazioni = true;
                self.scorri = 0.0;
                return Effetto::Ridisegna;
            }
            OpzAz::Gettone(k) => {
                let t = MODELLI[k as usize % MODELLI.len()];
                if !self.modello_nome.is_empty() && !self.modello_nome.ends_with(['_', '-', ' ']) {
                    self.modello_nome.push('_');
                }
                self.modello_nome.push_str(t);
                return Effetto::SalvaImpostazioni;
            }
            OpzAz::Esiste(k) => {
                self.esiste = [Exists::Number, Exists::Replace, Exists::Skip][k as usize % 3];
                return Effetto::SalvaImpostazioni;
            }
            OpzAz::Qualita(d) => {
                self.qualita = (self.qualita as i32 + d).clamp(40, 95) as u8;
                self.imp.quality = self.qualita;
                return Effetto::SalvaImpostazioni;
            }
            OpzAz::Dpi(d) => {
                self.imp.pdf_dpi = d;
                return Effetto::SalvaImpostazioni;
            }
            OpzAz::LinguaOcr(k) => {
                self.imp.ocr_lang = LINGUE_OCR[k as usize % 3].0.to_string();
                return Effetto::SalvaImpostazioni;
            }
            OpzAz::PagineOcr(d) => {
                // Da 10 a 500 a passi di 10; dopo 500 viene «Tutte» (0).
                let v = self.imp.ocr_max;
                self.imp.ocr_max = match (v, d > 0) {
                    (0, true) => 0,
                    (0, false) => 500,
                    (v, true) if v >= 500 => 0,
                    (v, true) => v + 10,
                    (v, false) => v.saturating_sub(10).max(10),
                };
                return Effetto::SalvaImpostazioni;
            }
        }
        Effetto::Ridisegna
    }

    // ---- Disegno ----

    fn barra_con(&mut self, p: &mut Painter, w: f32, titolo: &str, sotto: &str) {
        let pal = self.pal();
        self.bottone_icona(p, (4.0, 4.0, TOCCO, TOCCO), ic::ARROW_LEFT, Azione::Opz(OpzAz::Indietro));
        let sw = if sotto.is_empty() { 0.0 } else { p.measure(sotto, 14.0, Weight::Regular).min(w * 0.42) + 16.0 };
        p.text(titolo, 60.0, 0.0, w - 72.0 - sw, BARRA, 21.0, Weight::Semibold, pal.text, Align::Left);
        if !sotto.is_empty() {
            p.text(sotto, w - sw, 0.0, sw - 16.0, BARRA, 14.0, Weight::Regular, pal.muted, Align::Right);
        }
    }

    /// Gettoni che vanno a capo; restituisce l'altezza. `nuovi`: la scritta «NUOVO» accanto.
    #[allow(clippy::too_many_arguments)]
    fn gettoni(&mut self, p: &mut Painter, x: f32, y: f32, w: f32, voci: &[(String, bool)], scelta: Option<usize>, az: impl Fn(usize) -> Azione, vista: super::R) -> f32 {
        let pal = self.pal();
        let (mut cx, mut cy) = (x, y);
        for (i, (t, nuovo)) in voci.iter().enumerate() {
            let tw = p.measure(t, 15.0, Weight::Semibold) + 28.0 + if *nuovo { p.measure("NUOVO", 9.0, Weight::Semibold) + 6.0 } else { 0.0 };
            if cx + tw > x + w && cx > x {
                cx = x;
                cy += 50.0;
            }
            let on = scelta == Some(i);
            let a = az(i);
            let rc = (cx, cy, tw, 42.0);
            p.rrect(rc.0, rc.1, rc.2, rc.3, 21.0, Some(if on { pal.alt } else if self.premuto == Some(a) { mix(pal.surface, pal.primary, 0.08) } else { pal.surface }), Some((if on { pal.primary } else { pal.border }, if on { 2.0 } else { 1.5 })));
            let lw = p.measure(t, 15.0, Weight::Semibold);
            p.text(t, rc.0 + 14.0, rc.1, lw + 4.0, rc.3, 15.0, Weight::Semibold, if on { pal.primary } else { pal.text }, Align::Left);
            if *nuovo {
                p.text("NUOVO", rc.0 + 18.0 + lw, rc.1, rc.2 - lw - 20.0, rc.3, 9.0, Weight::Semibold, pal.ok, Align::Left);
            }
            self.colpo(rc, a, vista);
            cx += tw + 8.0;
        }
        cy + 42.0 - y
    }

    /// Una riga con meno, valore e più.
    #[allow(clippy::too_many_arguments)]
    fn passi(&mut self, p: &mut Painter, x: f32, y: f32, w: f32, titolo: &str, sotto: &str, valore: &str, meno: Azione, piu: Azione, vista: super::R) -> f32 {
        let pal = self.pal();
        let tw = w - 172.0;
        let righe = if sotto.is_empty() { 0 } else { p.wrap_lines(sotto, 13.0, Weight::Regular, tw, 2).len() };
        let dim = if p.measure(titolo, 16.0, Weight::Semibold) <= tw { 16.0 } else { 14.0 };
        let h = if sotto.is_empty() { 60.0 } else { 44.0 + 20.0 * righe as f32 };
        super::sollevato(p, pal, self.scuro(), x, y, w, h, 16.0);
        if sotto.is_empty() {
            p.text(titolo, x + 14.0, y, tw, h, dim, Weight::Semibold, pal.text, Align::Left);
        } else {
            p.text(titolo, x + 14.0, y + 8.0, tw, 26.0, dim, Weight::Semibold, pal.text, Align::Left);
            p.text_wrap(sotto, x + 14.0, y + 35.0, tw, 13.0, Weight::Regular, pal.muted, 20.0, 2);
        }
        let cy = y + h / 2.0;
        for (k, (t, a)) in [("−", meno), ("+", piu)].into_iter().enumerate() {
            let cx = x + w - 152.0 + k as f32 * 104.0;
            p.circle(cx + 18.0, cy, 18.0, Some(if self.premuto == Some(a) { pal.alt } else { pal.surface }), Some((pal.primary, 2.0)));
            p.text(t, cx, cy - 18.0, 36.0, 36.0, 18.0, Weight::Bold, pal.primary, Align::Center);
            self.colpo((cx - 6.0, cy - 24.0, 48.0, 48.0), a, vista);
        }
        p.text(valore, x + w - 118.0, cy - 16.0, 70.0, 32.0, 16.0, Weight::Semibold, pal.primary, Align::Center);
        h
    }

    pub(super) fn pagina_opzioni(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        let kind = self.kind_opz();
        let da_casa = self.opzioni.as_ref().is_some_and(|o| o.da_casa.is_some());
        let nome = if da_casa { conversioni::CATEGORIES.iter().find(|c| c.kind == kind).map(|c| c.name.to_string()).unwrap_or_default() } else { self.file.as_ref().map(|f| f.nome.clone()).unwrap_or_default() };
        self.barra_con(p, w, "Altre opzioni", &nome);
        let piede = 80.0;
        let vista = (0.0, BARRA, w, h - BARRA - piede);
        let ampio = w > 700.0;
        let cw = if ampio { 560.0 } else { w - 32.0 };
        let x = if ampio { (w - cw) / 2.0 } else { 16.0 };
        p.push_clip(vista.0, vista.1, vista.2, vista.3);
        self.pdf.campo_vis = None;
        let mut y = BARRA + 4.0 - self.scorri;
        let f = formati(kind);
        let scelto = self.opzioni.as_ref().map(|o| o.formato).unwrap_or(0);
        y += self.etichetta_opz(p, x, y, cw, "Formato");
        let voci: Vec<(String, bool)> = f.iter().map(|s| (s.label.to_string(), nuovo(s.id))).collect();
        y += self.gettoni(p, x, y, cw, &voci, Some(scelto), |i| Azione::Opz(OpzAz::Formato(i)), vista) + 12.0;
        let defs = self.defs();
        let valori = self.opzioni.as_ref().map(|o| o.valori.clone()).unwrap_or_default();
        for (k, d) in defs.iter().enumerate() {
            let v = valori.get(d);
            match &d.control {
                Control::Switch => {
                    y += self.riga_sw(p, x, y, cw, d.title, d.note, v != 0, Azione::Opz(OpzAz::Valore(k, (v == 0) as u32)), vista) + 8.0;
                }
                Control::Choice(scelte) | Control::Segments(scelte) | Control::Levels(scelte) => {
                    y += self.etichetta_opz(p, x, y, cw, d.title);
                    if scelte.len() <= 3 {
                        let nomi: Vec<&str> = scelte.iter().map(|c| c.label).collect();
                        let pos = scelte.iter().position(|c| c.value == v).unwrap_or(0);
                        let valori_scelte: Vec<u32> = scelte.iter().map(|c| c.value).collect();
                        self.pillole_in(p, (x, y, cw, TOCCO), &nomi, pos, |i| Azione::Opz(OpzAz::Valore(k, valori_scelte[i])), vista);
                        y += TOCCO + 6.0;
                    } else {
                        let voci: Vec<(String, bool)> = scelte.iter().map(|c| (c.label.to_string(), false)).collect();
                        let pos = scelte.iter().position(|c| c.value == v);
                        let valori_scelte: Vec<u32> = scelte.iter().map(|c| c.value).collect();
                        y += self.gettoni(p, x, y, cw, &voci, pos, |i| Azione::Opz(OpzAz::Valore(k, valori_scelte[i])), vista) + 6.0;
                    }
                    if !d.note.is_empty() {
                        y += p.text_wrap(d.note, x + 2.0, y, cw, 13.0, Weight::Regular, pal.muted, 18.0, 2) + 6.0;
                    }
                    y += 6.0;
                }
                Control::Range { min, max, unit } => {
                    let passo = if max - min > 60 { 5 } else { 1 };
                    y += self.passi(p, x, y, cw, d.title, d.note, &format!("{v}{unit}"), Azione::Opz(OpzAz::Passo(k, -passo)), Azione::Opz(OpzAz::Passo(k, passo)), vista) + 8.0;
                }
                Control::Text { hint } => {
                    y += self.etichetta_opz(p, x, y, cw, d.title);
                    let t = valori.text(d).to_string();
                    self.campo_testo(p, (x, y, cw, 52.0), Campo::Opzione(k), &t, hint, "", false, vista);
                    y += 58.0;
                }
                Control::Password => {
                    y += self.etichetta_opz(p, x, y, cw, d.title);
                    let t = valori.text(d).to_string();
                    self.campo_testo(p, (x, y, cw, 52.0), Campo::Opzione(k), &t, d.note, "", true, vista);
                    y += 58.0;
                }
                Control::Position => {
                    y += self.etichetta_opz(p, x, y, cw, d.title);
                    for q in 0..9u32 {
                        let (gx, gy) = (x + (q % 3) as f32 * 52.0, y + (q / 3) as f32 * 52.0);
                        let on = q == v;
                        p.rrect(gx, gy, 44.0, 44.0, 8.0, Some(if on { pal.primary } else { pal.alt }), Some((pal.border, 1.0)));
                        self.colpo((gx, gy, 48.0, 48.0), Azione::Opz(OpzAz::Valore(k, q)), vista);
                    }
                    y += 160.0;
                }
            }
        }
        let f_id = f.get(scelto).map(|s| s.id).unwrap_or("none");
        let errore = conversioni::validate(&valori, kind, f_id).err();
        if let Some(e) = &errore {
            y += p.text_wrap(e, x + 2.0, y, cw, 13.0, Weight::Regular, pal.bad, 18.0, 3) + 6.0;
        }
        y += p.text_wrap("Le stesse opzioni del pannello Conversioni del computer.", x + 2.0, y, cw, 13.0, Weight::Regular, pal.muted, 18.0, 2);
        p.pop_clip();
        self.scorri_max = (y + 16.0 + self.scorri - (vista.1 + vista.3)).max(0.0);
        self.scorri = self.scorri.min(self.scorri_max);
        self.tieni_campo(vista);
        let etichetta = if da_casa { "Fatto".to_string() } else { f.get(scelto).map(|s| format!("Converti in {}", s.label)).unwrap_or_default() };
        self.pulsante_opz(p, (x, h - piede + 12.0, cw, 56.0), &etichetta, errore.is_none().then_some(Azione::Opz(OpzAz::Vai)));
    }

    /// Le scelte fatte in «Altre opzioni» tornano alla finestra principale.
    fn da_opzioni(&mut self, k: Kind, formato: usize, valori: Values) {
        if let Some(s) = formati(k).get(formato) {
            self.casa.operazione.insert(super::casa::ck(k), s.id);
        }
        self.casa.valori.insert(super::casa::ck(k), valori);
        self.scorri = 0.0;
    }

    fn etichetta_opz(&self, p: &mut Painter, x: f32, y: f32, w: f32, t: &str) -> f32 {
        p.text(t, x + 2.0, y, w, 28.0, 14.0, Weight::Semibold, self.pal().muted, Align::Left);
        30.0
    }

    fn pulsante_opz(&mut self, p: &mut Painter, bt: super::R, testo: &str, az: Option<Azione>) {
        let pal = self.pal();
        let fondo = if az.is_none() { pal.alt } else if self.premuto == az { mix(pal.primary, (0, 0, 0), 0.15) } else { pal.primary };
        let fg = if az.is_some() { pal.on_primary } else { pal.muted };
        p.rrect(bt.0, bt.1, bt.2, bt.3, bt.3 / 2.0, Some(fondo), None);
        let tw = p.measure(testo, 18.0, Weight::Semibold).min(bt.2 - 70.0);
        let ix = bt.0 + (bt.2 - tw - 34.0) / 2.0;
        p.icon(ic::CONVERT, ix, bt.1 + (bt.3 - 24.0) / 2.0, 24.0, fg);
        p.text(testo, ix + 34.0, bt.1, tw + 4.0, bt.3, 18.0, Weight::Semibold, fg, Align::Left);
        if let Some(a) = az {
            self.colpi.push((super::Forma::R(bt), a));
        }
    }

    pub(super) fn pagina_impostazioni(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        self.barra_con(p, w, "Impostazioni", "");
        let vista = (0.0, BARRA, w, h - BARRA);
        let ampio = w > 700.0;
        let cw = if ampio { 560.0 } else { w - 32.0 };
        let x = if ampio { (w - cw) / 2.0 } else { 16.0 };
        p.push_clip(vista.0, vista.1, vista.2, vista.3);
        self.pdf.campo_vis = None;
        let mut y = BARRA + 4.0 - self.scorri;
        y += self.etichetta_opz(p, x, y, cw, "Nome del file nuovo");
        let m = self.modello_nome.clone();
        self.campo_testo(p, (x, y, cw, 52.0), Campo::Modello, &m, "Come l'originale", "", false, vista);
        y += 60.0;
        let voci: Vec<(String, bool)> = MODELLI.iter().map(|t| (t.to_string(), false)).collect();
        y += self.gettoni(p, x, y, cw, &voci, None, |i| Azione::Opz(OpzAz::Gettone(i as u8)), vista) + 6.0;
        let esempio = cedro::name_example(&m, "verbale", "pdf").map(|n| format!("{n}.pdf")).unwrap_or_else(|| "verbale.pdf".into());
        let ew = p.measure("Esempio: ", 14.0, Weight::Regular);
        p.text("Esempio: ", x + 2.0, y, ew, 24.0, 14.0, Weight::Regular, pal.muted, Align::Left);
        p.text(&esempio, x + 2.0 + ew, y, cw - ew, 24.0, 14.0, Weight::Semibold, pal.text, Align::Left);
        y += 34.0;
        y += self.etichetta_opz(p, x, y, cw, "Se il file esiste già");
        let k = match self.esiste {
            Exists::Replace => 1,
            Exists::Skip => 2,
            _ => 0,
        };
        self.pillole_in(p, (x, y, cw, TOCCO), &["Numera -2, -3", "Sostituisci", "Salta"], k, |i| Azione::Opz(OpzAz::Esiste(i as u8)), vista);
        y += TOCCO + 12.0;
        let q = self.qualita.to_string();
        y += self.passi(p, x, y, cw, "Qualità predefinita", "JPEG, WebP, MP3, video", &q, Azione::Opz(OpzAz::Qualita(-1)), Azione::Opz(OpzAz::Qualita(1)), vista) + 8.0;
        y += self.etichetta_opz(p, x, y, cw, "Risoluzione delle pagine PDF");
        let voci: Vec<(String, bool)> = DPI.iter().map(|d| (format!("{d} dpi"), false)).collect();
        let pos = DPI.iter().position(|d| *d == self.imp.pdf_dpi);
        y += self.gettoni(p, x, y, cw, &voci, pos, |i| Azione::Opz(OpzAz::Dpi(DPI[i])), vista);
        y += p.text_wrap("PDF in JPG e PNG", x + 2.0, y + 4.0, cw, 13.0, Weight::Regular, pal.muted, 18.0, 1) + 14.0;
        y += self.etichetta_opz(p, x, y, cw, "Lingua dell'OCR");
        let lk = LINGUE_OCR.iter().position(|(c, _)| *c == self.imp.ocr_lang).unwrap_or(0);
        let nomi: Vec<&str> = LINGUE_OCR.iter().map(|(_, n)| *n).collect();
        self.pillole_in(p, (x, y, cw, TOCCO), &nomi, lk, |i| Azione::Opz(OpzAz::LinguaOcr(i as u8)), vista);
        y += TOCCO + 12.0;
        let n = if self.imp.ocr_max == 0 { "Tutte".to_string() } else { self.imp.ocr_max.to_string() };
        y += self.passi(p, x, y, cw, "Pagine massime per l'OCR", "Oltre, dice quali ha lasciato fuori", &n, Azione::Opz(OpzAz::PagineOcr(-10)), Azione::Opz(OpzAz::PagineOcr(10)), vista) + 8.0;
        super::sollevato(p, pal, self.scuro(), x, y, cw, 64.0, 16.0);
        p.text("Dove salvare", x + 14.0, y + 8.0, cw - 28.0, 26.0, 16.0, Weight::Semibold, pal.text, Align::Left);
        p.text("Documenti/Cedro, sul telefono", x + 14.0, y + 34.0, cw - 28.0, 20.0, 13.0, Weight::Regular, pal.muted, Align::Left);
        y += 72.0;
        p.pop_clip();
        self.scorri_max = (y + 16.0 + self.scorri - h).max(0.0);
        self.scorri = self.scorri.min(self.scorri_max);
        self.tieni_campo(vista);
    }
}
