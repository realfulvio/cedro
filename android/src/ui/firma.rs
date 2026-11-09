//! Firma digitale, come sul computer (0.37): la sezione nella finestra principale (apri e verifica,
//! ultimi file, revoca online, elenco UE) e il visualizzatore di un file firmato (.p7m, .p7s, .m7m,
//! .tsd, .tsr, PDF firmati). La verifica è quella del motore (`cedro::firma`), con lo stato del
//! computer (`cedro::ui::firma::FirmaUi`): qui c'è solo il disegno per il telefono e i tocchi.
//! «Verifica una cartella» resta sul computer: sul telefono i file arrivano uno alla volta.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use cedro::firma::{self, At, DocKind, Level, Report};
use cedro::ui::gfx::{Align, Fit, Image, Painter, Weight};
use cedro::ui::theme::mix;

use super::{ic, App, Azione, Effetto, Forma, Lavoro, Sorgente, Stato, R, TOCCO};

type Rgb = (u8, u8, u8);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FirmaAz {
    /// «Apri e verifica»: il selettore di Android.
    Scegli,
    Recente(usize),
    Online,
    Elenco,
    Chiudi,
    /// Apre o chiude una firma.
    Firma(usize),
    Tecnici(usize),
    /// Verifica alla data: 0 oggi, 1 quando è stato firmato.
    Alla(u8),
    Salva,
    Fuori,
    Condividi,
    Rapporto,
    Originale,
    Versione(usize),
    Torna,
}

/// Le pagine del documento mostrato nel visualizzatore, preparate da `glue` in un thread.
#[derive(Default)]
pub struct Pagine {
    pub di: Option<PathBuf>,
    pub img: Vec<Image>,
    pub finite: bool,
    /// Da preparare: il documento (estratto o la versione firmata).
    pub chiedi: Option<PathBuf>,
    /// I file già scritti in Documenti/Cedro: (che cosa, è il rapporto) → lavoro.
    pub salvati: Vec<(PathBuf, bool, u32)>,
}

/// Il colore di un giudizio (l'arancio degli avvisi è quello del computer).
pub fn colore_livello(app: &App, l: Level) -> Rgb {
    let pal = app.pal();
    match l {
        Level::Ok => pal.ok,
        Level::Warn => {
            if app.tema.dark() {
                (0xF0, 0xB4, 0x4C)
            } else {
                (0xA8, 0x5A, 0x00)
            }
        }
        Level::Bad => pal.bad,
    }
}

fn segno(l: Level) -> &'static str {
    match l {
        Level::Ok => ic::CHECK,
        Level::Warn => "M12 6v8M12 18v.1",
        Level::Bad => ic::CLOSE,
    }
}

fn nome_file(p: &Path) -> String {
    p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default()
}

/// Il nome da mostrare.
fn nome_visto(p: &Path) -> String {
    nome_file(p)
}

impl App {
    /// Il file è da aprire nel visualizzatore delle firme (dal nome: .p7m, .p7s, .m7m, .tsd, .tsr, .p7e).
    pub fn e_firmato(nome: &str) -> bool {
        firma::is_signed_name(&nome.to_ascii_lowercase())
    }

    /// Apre il visualizzatore su un file (condiviso, scelto o tra gli ultimi).
    pub fn apri_firmato(&mut self, path: PathBuf) {
        self.fpag = Pagine { salvati: std::mem::take(&mut self.fpag.salvati), ..Default::default() };
        self.scorri = 0.0;
        self.firma.open_viewer(path, self.sveglia.clone());
    }

    /// Il file originale di una firma staccata (.p7s) o di una marca a parte (.tsr): si rifà la verifica.
    pub fn originale_firmato(&mut self, dati: Vec<u8>) {
        let at = self.firma.viewer.as_ref().map(|v| v.at).unwrap_or(At::Now);
        self.firma.reverify(at, Some(dati), self.sveglia.clone());
    }

    /// Dopo la verifica: il documento da mostrare va preparato?
    pub fn firma_arrivi(&mut self) -> bool {
        let cambiato = self.firma.pump();
        let mostrato = self.firma.viewer.as_ref().and_then(|v| match &v.revision {
            Some((_, p)) => Some(p.clone()),
            None => v.doc_path.clone(),
        });
        if mostrato.is_some() && mostrato != self.fpag.di {
            self.fpag.di = mostrato.clone();
            self.fpag.img.clear();
            self.fpag.finite = false;
            self.fpag.chiedi = mostrato;
        }
        cambiato
    }

    /// Una pagina pronta (`None`: finite).
    pub fn firma_pagina(&mut self, di: &Path, img: Option<Image>) {
        if self.fpag.di.as_deref() != Some(di) {
            return;
        }
        match img {
            Some(i) => self.fpag.img.push(i),
            None => self.fpag.finite = true,
        }
    }

    pub(super) fn azione_firma(&mut self, az: FirmaAz) -> Effetto {
        match az {
            FirmaAz::Scegli => Effetto::Scegli(Sorgente::Firmati),
            FirmaAz::Recente(i) => {
                if let Some(r) = self.firma.recent.get(i).cloned() {
                    if r.path.exists() {
                        self.apri_firmato(r.path);
                    } else {
                        // Il file non c'è più (copia di una condivisione tolta): si toglie dall'elenco.
                        self.firma.recent.remove(i);
                    }
                }
                Effetto::Ridisegna
            }
            FirmaAz::Online => {
                self.imp.firma_online = !self.imp.firma_online;
                self.firma.opts.2 = self.imp.firma_online;
                Effetto::SalvaImpostazioni
            }
            FirmaAz::Elenco => {
                self.firma.update_trust(self.sveglia.clone());
                Effetto::Ridisegna
            }
            FirmaAz::Chiudi => {
                self.firma.viewer = None;
                self.fpag.di = None;
                self.fpag.img.clear();
                self.scorri = 0.0;
                Effetto::Ridisegna
            }
            FirmaAz::Firma(i) => {
                if let Some(v) = self.firma.viewer.as_mut() {
                    if !v.open.remove(&i) {
                        v.open.insert(i);
                    }
                }
                Effetto::Ridisegna
            }
            FirmaAz::Tecnici(i) => {
                if let Some(v) = self.firma.viewer.as_mut() {
                    if !v.tech.remove(&i) {
                        v.tech.insert(i);
                    }
                }
                Effetto::Ridisegna
            }
            FirmaAz::Alla(k) => {
                let at = if k == 1 { At::Signing } else { At::Now };
                if self.firma.viewer.as_ref().is_some_and(|v| v.at != at) {
                    self.firma.reverify(at, None, self.sveglia.clone());
                }
                Effetto::Ridisegna
            }
            FirmaAz::Originale => Effetto::Scegli(Sorgente::Originale),
            FirmaAz::Versione(i) => {
                let Some(v) = self.firma.viewer.as_mut() else { return Effetto::Niente };
                let Some(rep) = v.report.clone() else { return Effetto::Niente };
                if let Some(b) = rep.signed_revision(i) {
                    let nome = format!("{} (versione firmata).pdf", Path::new(&nome_visto(&v.path)).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default());
                    if let Some(p) = cedro::ui::firma::write_temp(&nome, b) {
                        v.revision = Some((i, p));
                        self.scorri = 0.0;
                    }
                }
                self.firma_arrivi();
                Effetto::Ridisegna
            }
            FirmaAz::Torna => {
                if let Some(v) = self.firma.viewer.as_mut() {
                    v.revision = None;
                }
                self.scorri = 0.0;
                self.firma_arrivi();
                Effetto::Ridisegna
            }
            FirmaAz::Salva => self.scrivi_firma(false, 0),
            FirmaAz::Fuori => self.scrivi_firma(false, 1),
            FirmaAz::Condividi => self.scrivi_firma(false, 2),
            FirmaAz::Rapporto => self.scrivi_firma(true, 1),
        }
    }

    /// Che cosa salvare del file nel visualizzatore: nome e byte (il documento, la versione firmata o il file stesso).
    fn da_salvare(&self) -> Option<(PathBuf, String, Vec<u8>)> {
        let v = self.firma.viewer.as_ref()?;
        let rep = v.report.as_ref()?;
        if let Some((_, p)) = &v.revision {
            return Some((p.clone(), nome_file(p), std::fs::read(p).ok()?));
        }
        if proprio_pdf(&v.path) || rep.doc.is_empty() {
            return Some((v.path.clone(), nome_visto(&v.path), std::fs::read(&v.path).ok()?));
        }
        let nome = if rep.doc_name.trim().is_empty() { "documento".to_string() } else { rep.doc_name.clone() };
        Some((v.path.clone(), nome, rep.doc.clone()))
    }

    /// Scrive in Documenti/Cedro il documento (o il rapporto) e poi, se serve, lo apre (1) o lo condivide (2).
    /// Una seconda volta si riusa il file già scritto.
    fn scrivi_firma(&mut self, rapporto: bool, poi: u8) -> Effetto {
        let Some(v) = self.firma.viewer.as_ref() else { return Effetto::Niente };
        let Some(rep) = v.report.clone() else { return Effetto::Niente };
        let (chiave, nome, dati) = if rapporto {
            let dati = match firma::rapporto::pdf(&[&rep]) {
                Ok(d) => d,
                Err(e) => {
                    log::error!("rapporto: {e}");
                    return Effetto::Niente;
                }
            };
            let stem = Path::new(&nome_visto(&v.path)).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
            (v.path.clone(), format!("Verifica firme - {stem}.pdf"), dati)
        } else {
            match self.da_salvare() {
                Some(t) => t,
                None => return Effetto::Niente,
            }
        };
        let gia = self.fpag.salvati.iter().find(|(p, r, _)| *p == chiave && *r == rapporto).map(|t| t.2);
        if let Some(id) = gia {
            if let Some(Stato::Fatto { path, .. }) = self.lavori.iter().find(|l| l.id == id).map(|l| l.stato.clone()) {
                return match poi {
                    1 => Effetto::Apri(path),
                    2 => Effetto::Condividi(path),
                    _ => {
                        self.sopra = Some(super::Scheda::Lavori);
                        self.casa_aperta = true;
                        Effetto::Ridisegna
                    }
                };
            }
        }
        let id = self.prossimo;
        self.prossimo += 1;
        let et = if rapporto { "Rapporto" } else { "Documento" };
        self.lavori.insert(0, Lavoro { id, origine: nome_visto(&v.path), etichetta: et.into(), conv: false, inizio: std::time::Instant::now(), quando: super::adesso(), stato: Stato::InCorso, prog: Arc::new(std::sync::atomic::AtomicU32::new(u32::MAX)), sorgente: None, controlli: None });
        self.fpag.salvati.push((chiave, rapporto, id));
        Effetto::ScriviPoi { id, nome, dati, poi }
    }

    // ---- La sezione nella finestra principale ----

    /// Il corpo della sezione «Firma digitale» (al posto della coda dei file). Restituisce l'altezza.
    pub(super) fn corpo_firma(&mut self, p: &mut Painter, x: f32, y0: f32, w: f32, vista: R) -> f32 {
        let pal = self.pal();
        let scuro = self.tema.dark();
        let viola = super::casa::colore_firma(scuro);
        let mut y = y0;
        // Apri e verifica.
        let testo = "Apri un .p7m o un PDF firmato: vedi il documento, chi l'ha firmato e se la firma vale.";
        let th = p.wrap_height(testo, 14.0, Weight::Regular, w - 116.0, 20.0, 4);
        let ch = (th + 52.0).max(84.0);
        let a = Azione::Firma(FirmaAz::Scegli);
        super::sollevato(p, pal, scuro, x, y, w, ch, 12.0);
        if self.premuto == Some(a) {
            p.rrect(x, y, w, ch, 12.0, Some(pal.alt), None);
        }
        super::piastrella(p, pal, scuro, x + 14.0, y + 18.0, 48.0, ic::SHIELD, viola, false);
        p.text("Apri e verifica", x + 76.0, y + 12.0, w - 110.0, 28.0, 17.0, Weight::Semibold, pal.text, Align::Left);
        p.text_wrap(testo, x + 76.0, y + 40.0, w - 116.0, 14.0, Weight::Regular, pal.muted, 20.0, 4);
        p.icon("M9 6l6 6-6 6", x + w - 28.0, y + ch / 2.0 - 10.0, 20.0, viola);
        self.colpo((x, y, w, ch), a, vista);
        y += ch + 10.0;
        y += p.text_wrap("Funziona anche con gli allegati firmati: aprili dalla mail o da WhatsApp e scegli Cedro.", x + 4.0, y, w - 8.0, 13.0, Weight::Regular, pal.muted, 18.0, 3) + 16.0;
        // Ultimi file.
        p.text("Ultimi file", x + 2.0, y, w, 24.0, 15.0, Weight::Semibold, pal.muted, Align::Left);
        y += 30.0;
        let recenti = self.firma.recent.clone();
        if recenti.is_empty() {
            p.rrect_line(x, y, w, 64.0, 10.0, pal.border, 1.5, 1.0, Some((6.0, 5.0)));
            p.text_wrap("Qui ritrovi i file firmati che apri, con il loro giudizio.", x + 16.0, y + 12.0, w - 32.0, 14.0, Weight::Regular, pal.muted, 20.0, 2);
            y += 76.0;
        } else {
            let rh = 60.0;
            super::sollevato(p, pal, scuro, x, y, w, rh * recenti.len() as f32, 10.0);
            for (k, r) in recenti.iter().enumerate() {
                let ry = y + k as f32 * rh;
                let a = Azione::Firma(FirmaAz::Recente(k));
                if self.premuto == Some(a) {
                    p.rrect(x + 1.0, ry + 1.0, w - 2.0, rh - 2.0, 9.0, Some(pal.alt), None);
                }
                if k > 0 {
                    p.fill(x + 14.0, ry, w - 28.0, 1.0, pal.border);
                }
                let c = colore_livello(self, r.level);
                p.circle(x + 28.0, ry + rh / 2.0, 13.0, Some(c), None);
                p.icon(segno(r.level), x + 19.0, ry + rh / 2.0 - 9.0, 18.0, (255, 255, 255));
                p.text(&nome_visto(&r.path), x + 52.0, ry + 8.0, w - 64.0, 24.0, 15.0, Weight::Semibold, pal.text, Align::Left);
                let sotto = format!("{} · {}", r.headline, firma::relative(r.when));
                p.text(&sotto, x + 52.0, ry + 32.0, w - 64.0, 20.0, 12.5, Weight::Regular, pal.muted, Align::Left);
                self.colpo((x, ry, w, rh), a, vista);
            }
            y += rh * recenti.len() as f32 + 16.0;
        }
        // Le opzioni: revoca online ed elenco dei certificatori.
        let sotto1 = "Chiedo al certificatore se il certificato è stato revocato";
        let h1 = 44.0 + p.wrap_height(sotto1, 12.5, Weight::Regular, w - 96.0, 17.0, 3) + 12.0;
        let (quando, quanti) = self.firma.tl_info;
        let riga = match quando {
            Some(t) if quanti > 0 => format!("Aggiornato {} · {quanti} certificatori", firma::relative(t)),
            _ => "Non ancora scaricato: lo scarico alla prima verifica".to_string(),
        };
        let h2 = 44.0 + p.wrap_height(&riga, 12.5, Weight::Regular, w - 136.0, 17.0, 3).max(40.0) + 12.0;
        let oh = h1 + h2;
        super::sollevato(p, pal, scuro, x, y, w, oh, 10.0);
        let a = Azione::Firma(FirmaAz::Online);
        p.text("Controlla online la revoca", x + 14.0, y + 12.0, w - 96.0, 24.0, 15.0, Weight::Semibold, pal.text, Align::Left);
        p.text_wrap(sotto1, x + 14.0, y + 38.0, w - 96.0, 12.5, Weight::Regular, pal.muted, 17.0, 3);
        let on = self.imp.firma_online;
        let t = self.anim.verso(super::anim::Chiave::Interruttore(1), if on { 1.0 } else { 0.0 }, 160.0);
        interruttore(p, pal, x + w - 60.0, y + h1 / 2.0, t);
        self.colpo((x, y, w, h1), a, vista);
        p.fill(x + 14.0, y + h1, w - 28.0, 1.0, pal.border);
        let y2 = y + h1;
        p.text("Elenco europeo dei certificatori", x + 14.0, y2 + 12.0, w - 28.0, 24.0, 15.0, Weight::Semibold, pal.text, Align::Left);
        p.text_wrap(&riga, x + 14.0, y2 + 38.0, w - 136.0, 12.5, Weight::Regular, pal.muted, 17.0, 3);
        let b = (x + w - 112.0, y2 + 38.0, 98.0, 40.0);
        let a = Azione::Firma(FirmaAz::Elenco);
        let occupato = self.firma.tl_busy;
        p.rrect(b.0, b.1, b.2, b.3, 8.0, Some(if self.premuto == Some(a) { pal.alt } else { pal.surface }), Some((pal.border, 1.0)));
        p.text(if occupato { "Scarico…" } else { "Aggiorna" }, b.0, b.1, b.2, b.3, 14.0, Weight::Semibold, if occupato { pal.muted } else { pal.primary }, Align::Center);
        if !occupato {
            self.colpo(b, a, vista);
        }
        y += oh + 12.0;
        y - y0
    }

    // ---- Il visualizzatore ----

    pub(super) fn pagina_firma(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        let scuro = self.tema.dark();
        let Some(v) = self.firma.viewer.as_ref() else { return };
        let nome = nome_visto(&v.path);
        let rep = v.report.clone();
        let controllo = v.checking;
        let revisione = v.revision.as_ref().map(|r| r.0);
        let at = v.at;
        let aperte = v.open.clone();
        let tecnici = v.tech.clone();
        // Entra da destra.
        let entra = self.anim.comparsa(super::anim::Chiave::Pagina, 220.0);
        p.dx += (1.0 - super::anim::esci(entra)) * w * 0.25;
        // Barra in alto.
        self.bottone_icona(p, (4.0, 4.0, TOCCO, TOCCO), ic::ARROW_LEFT, Azione::Firma(FirmaAz::Chiudi));
        p.text(&nome, 60.0, 0.0, w - 72.0, 56.0, 17.0, Weight::Semibold, pal.text, Align::Left);
        let mut top = 56.0;
        // La fascia col giudizio.
        match &rep {
            Some(r) => {
                let c = if r.unsigned() { pal.muted } else { colore_livello(self, r.level) };
                let tw = w - 64.0;
                let dh = if r.detail.is_empty() { 0.0 } else { p.wrap_height(&r.detail, 13.5, Weight::Regular, tw, 19.0, 4) };
                let fh = 20.0 + 24.0 + dh + 6.0;
                p.fill(-self.margini.0, top, w + self.margini.0 + self.margini.2, fh, mix(c, pal.surface, if scuro { 0.82 } else { 0.9 }));
                p.fill(-self.margini.0, top + fh - 2.0, w + self.margini.0 + self.margini.2, 2.0, c);
                p.circle(28.0, top + 24.0, 14.0, Some(c), None);
                p.icon(if r.unsigned() { ic::FILE } else { segno(r.level) }, 19.0, top + 15.0, 18.0, (255, 255, 255));
                p.text(&r.headline, 52.0, top + 10.0, tw, 26.0, 16.0, Weight::Bold, c, Align::Left);
                if dh > 0.0 {
                    p.text_wrap(&r.detail, 52.0, top + 36.0, tw, 13.5, Weight::Regular, pal.text, 19.0, 4);
                }
                top += fh;
            }
            None => {
                p.fill(-self.margini.0, top, w + self.margini.0 + self.margini.2, 52.0, pal.alt);
                rotella(p, self, 28.0, top + 26.0, pal.primary);
                p.text("Controllo le firme…", 52.0, top, w - 64.0, 52.0, 15.0, Weight::Semibold, pal.text, Align::Left);
                top += 52.0;
            }
        }
        // Le azioni in basso.
        let ab = 68.0;
        let ay = h - ab;
        let vista = (0.0, top, w, ay - top);
        p.push_clip(vista.0, vista.1, vista.2, vista.3);
        let x = 16.0;
        let cw = w - 32.0;
        let mut y = top + 14.0 - self.scorri;
        if let Some(r) = rep.as_ref() {
            // Sul telefono prima il giudizio su ogni firma, poi il documento.
            if controllo && !r.signers.is_empty() {
                y += p.text_wrap("Controllo la revoca online…", x, y, cw, 13.0, Weight::Regular, pal.muted, 18.0, 1) + 8.0;
            }
            y = self.firme(p, r, &aperte, &tecnici, revisione, x, y, cw, vista);
            if !r.signers.is_empty() || r.needs_original || !r.doc.is_empty() {
                let titolo = if r.doc_name.trim().is_empty() || proprio_pdf(&self.firma.viewer.as_ref().map(|v| v.path.clone()).unwrap_or_default()) { "Documento".to_string() } else { format!("Documento: {}", r.doc_name) };
                p.text(&titolo, x, y, cw, 28.0, 18.0, Weight::Semibold, pal.text, Align::Left);
                y += 36.0;
            }
            y = self.documento_firmato(p, r, revisione, &aperte, x, y, cw, vista);
            if !r.signers.is_empty() {
                y = self.alla_data(p, at, x, y, cw, vista);
            }
        }
        p.pop_clip();
        self.scorri_max = (y + self.scorri - (vista.1 + vista.3) + 12.0).max(0.0);
        self.scorri = self.scorri.min(self.scorri_max);
        // La barra delle azioni.
        p.fill(-self.margini.0, ay, w + self.margini.0 + self.margini.2, ab + self.margini.3, pal.surface);
        p.fill(-self.margini.0, ay, w + self.margini.0 + self.margini.2, 1.0, pal.border);
        let Some(r) = rep else { return };
        let path = self.firma.viewer.as_ref().map(|v| v.path.clone()).unwrap_or_default();
        let mut azioni: Vec<(String, &'static str, FirmaAz, bool)> = Vec::new();
        let ha_doc = !r.doc.is_empty();
        if !r.unsigned() && ha_doc && (!proprio_pdf(&path) || revisione.is_some()) {
            let ext = Path::new(&r.doc_name).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
            let t = if revisione.is_some() {
                "Salva questa versione".to_string()
            } else if ext.is_empty() {
                "Salva".to_string()
            } else {
                format!("Salva il .{ext}")
            };
            azioni.push((t, ic::DOWNLOAD, FirmaAz::Salva, true));
        }
        if ha_doc || proprio_pdf(&path) {
            azioni.push(("Apri".into(), ic::OPEN_OUT, FirmaAz::Fuori, false));
            azioni.push(("Condividi".into(), I_CONDIVIDI, FirmaAz::Condividi, false));
        }
        if !r.unsigned() && !r.signers.is_empty() {
            azioni.push(("Rapporto".into(), ic::FILE, FirmaAz::Rapporto, false));
        }
        if azioni.is_empty() {
            return;
        }
        let bw = w / azioni.len() as f32;
        let viola = super::casa::colore_firma(scuro);
        for (k, (t, i, az, primo)) in azioni.into_iter().enumerate() {
            let rc = (k as f32 * bw, ay + 4.0, bw, ab - 8.0);
            let a = Azione::Firma(az);
            if self.premuto == Some(a) {
                p.rrect(rc.0 + 4.0, rc.1, rc.2 - 8.0, rc.3, 12.0, Some(pal.alt), None);
            }
            let c = if primo { viola } else { pal.text };
            p.icon(i, rc.0 + rc.2 / 2.0 - 12.0, rc.1 + 6.0, 24.0, c);
            p.text(&t, rc.0 + 2.0, rc.1 + 34.0, rc.2 - 4.0, 20.0, 12.5, if primo { Weight::Semibold } else { Weight::Regular }, c, Align::Center);
            self.colpi.push((Forma::R(rc), a));
        }
    }

    /// Il documento: le pagine una sotto l'altra, oppure un riquadro che spiega perché non si vede.
    #[allow(clippy::too_many_arguments)]
    fn documento_firmato(&mut self, p: &mut Painter, r: &Report, revisione: Option<usize>, aperte: &std::collections::HashSet<usize>, x: f32, mut y: f32, w: f32, vista: R) -> f32 {
        let pal = self.pal();
        let riquadro = |p: &mut Painter, icona: &str, titolo: &str, testo: &str, y: f32| -> f32 {
            let th = p.wrap_height(testo, 14.0, Weight::Regular, w - 32.0, 20.0, 8);
            let h = 90.0 + th;
            p.rrect(x, y, w, h, 12.0, Some(pal.surface), Some((pal.border, 1.0)));
            p.icon(icona, x + w / 2.0 - 16.0, y + 14.0, 32.0, pal.muted);
            p.text(titolo, x + 16.0, y + 50.0, w - 32.0, 24.0, 16.0, Weight::Semibold, pal.text, Align::Center);
            p.text_wrap(testo, x + 16.0, y + 78.0, w - 32.0, 14.0, Weight::Regular, pal.muted, 20.0, 8);
            h
        };
        if r.needs_original {
            let h = riquadro(p, ic::FILE, "Manca il file originale", "È una firma staccata (.p7s) o una marca a parte (.tsr): la controllo insieme al file che è stato firmato.", y);
            y += h + 8.0;
            let b = (x, y, w, 48.0);
            let a = Azione::Firma(FirmaAz::Originale);
            super::lucido(p, self.tema.dark(), b.0, b.1, b.2, b.3, 10.0, pal.primary, self.premuto == Some(a));
            p.text("Scegli il file originale", b.0, b.1, b.2, b.3, 16.0, Weight::Semibold, pal.on_primary, Align::Center);
            self.colpo(b, a, vista);
            return y + 64.0;
        }
        if r.doc.is_empty() {
            let t = if r.error.is_some() { "Non c'è un documento da mostrare" } else { "Il file è vuoto" };
            return y + riquadro(p, ic::WARNING, t, &r.detail.clone(), y) + 16.0;
        }
        if let Some(i) = revisione {
            let chi = r.signers.get(i).map(|s| firma::title_case(&s.name)).unwrap_or_default();
            let t = format!("Versione firmata da {chi}: com'era al momento della firma");
            let th = p.wrap_height(&t, 13.5, Weight::Semibold, w - 130.0, 18.0, 3);
            let bh = th + 20.0;
            p.rrect(x, y, w, bh, 12.0, Some(pal.text), None);
            p.text_wrap(&t, x + 14.0, y + 10.0, w - 130.0, 13.5, Weight::Semibold, pal.surface, 18.0, 3);
            let a = Azione::Firma(FirmaAz::Torna);
            p.text("Torna al file", x + w - 116.0, y, 104.0, bh, 14.0, Weight::Bold, mix(pal.surface, pal.primary, 0.3), Align::Right);
            self.colpo((x + w - 124.0, y, 124.0, bh), a, vista);
            y += bh + 12.0;
        }
        let kind = r.doc_kind.unwrap_or(DocKind::Other);
        if matches!(kind, DocKind::Other | DocKind::None) {
            let t = format!("Dentro la busta c'è «{}» ({}). La firma è controllata lo stesso: per vederlo aprilo con un'altra app.", r.doc_name, cedro::size_words(r.doc.len() as u64));
            return y + riquadro(p, ic::FILE, "Questo tipo di file non lo so mostrare", &t, y) + 16.0;
        }
        // Un testo si legge meglio come testo che come pagina A4 rimpicciolita.
        if matches!(kind, DocKind::Text | DocKind::Xml) && revisione.is_none() {
            if let Ok(t) = std::str::from_utf8(&r.doc) {
                let righe: Vec<&str> = t.lines().take(400).collect();
                let mut alto = 28.0;
                for l in &righe {
                    alto += p.wrap_height(if l.is_empty() { " " } else { l }, 14.0, Weight::Regular, w - 32.0, 20.0, 40).max(20.0);
                }
                p.shadow(x, y, w, alto, 4.0, 10.0, 0.14, 3.0);
                p.rrect(x, y, w, alto, 4.0, Some((255, 255, 255)), None);
                let mut ty = y + 14.0;
                for l in &righe {
                    let lh = p.wrap_height(if l.is_empty() { " " } else { l }, 14.0, Weight::Regular, w - 32.0, 20.0, 40).max(20.0);
                    if ty + lh > vista.1 && ty < vista.1 + vista.3 && !l.is_empty() {
                        p.text_wrap(l, x + 16.0, ty, w - 32.0, 14.0, Weight::Regular, (34, 38, 44), 20.0, 40);
                    }
                    ty += lh;
                }
                return y + alto + 20.0;
            }
        }
        if self.fpag.img.is_empty() {
            if self.fpag.finite {
                let t = format!("«{}» ({}): aprilo con un'altra app per vederlo.", r.doc_name, cedro::size_words(r.doc.len() as u64));
                return y + riquadro(p, ic::FILE, "Non riesco a mostrare il documento", &t, y) + 16.0;
            }
            let ph = w * 1.414;
            p.shadow(x, y, w, ph, 2.0, 10.0, 0.16, 3.0);
            p.fill(x, y, w, ph, (255, 255, 255));
            rotella(p, self, x + w / 2.0, y + 80.0, pal.primary);
            p.text("Preparo l'anteprima…", x, y + 100.0, w, 24.0, 14.0, Weight::Regular, (90, 96, 104), Align::Center);
            return y + ph + 20.0;
        }
        let pagine = self.fpag.img.len();
        for n in 0..pagine {
            let (iw, ih) = (self.fpag.img[n].w as f32, self.fpag.img[n].h as f32);
            let ph = w * ih / iw.max(1.0);
            if y + ph > vista.1 && y < vista.1 + vista.3 {
                p.shadow(x, y, w, ph, 2.0, 10.0, 0.16, 3.0);
                p.fill(x, y, w, ph, (255, 255, 255));
                let img = &self.fpag.img[n];
                p.image(img, x, y, w, ph, 0.0, Fit::Contain);
                // Il riquadro delle firme aperte che stanno su questa pagina, nel colore del giudizio.
                for (i, s) in r.signers.iter().enumerate() {
                    let aperta = aperte.contains(&i) || r.signers.len() == 1;
                    let Some(pl) = s.place.as_ref().filter(|pl| aperta && pl.visible && pl.page == Some(n as u32 + 1)) else { continue };
                    let Some(rc) = pl.rect else { continue };
                    let c = colore_livello(self, s.level);
                    let (x0, y0) = (x + rc[0] * w - 3.0, y + rc[1] * ph - 3.0);
                    p.rrect(x0, y0, (rc[2] - rc[0]) * w + 6.0, (rc[3] - rc[1]) * ph + 6.0, 5.0, None, Some((c, 2.0)));
                }
            }
            y += ph + 12.0;
        }
        if !self.fpag.finite {
            rotella(p, self, x + w / 2.0, y + 14.0, pal.primary);
            y += 36.0;
        } else if pagine > 1 {
            p.text(&format!("{pagine} pagine"), x, y - 4.0, w, 20.0, 12.5, Weight::Regular, pal.muted, Align::Center);
            y += 20.0;
        }
        y + 8.0
    }

    /// Chi ha firmato: una scheda per firma, con i quattro controlli quando è aperta; poi le marche.
    #[allow(clippy::too_many_arguments)]
    fn firme(&mut self, p: &mut Painter, r: &Report, aperte: &std::collections::HashSet<usize>, tecnici: &std::collections::HashSet<usize>, revisione: Option<usize>, x: f32, mut y: f32, w: f32, vista: R) -> f32 {
        let pal = self.pal();
        let scuro = self.tema.dark();
        if !r.signers.is_empty() {
            p.text(if r.signers.len() > 1 { "Chi ha firmato" } else { "Firma" }, x, y, w, 28.0, 18.0, Weight::Semibold, pal.text, Align::Left);
            p.text(&format!("controllato {}", firma::relative(r.checked_at).replace("oggi alle", "oggi,")), x, y, w, 28.0, 12.5, Weight::Regular, pal.muted, Align::Right);
            y += 36.0;
        }
        for i in r.ordered() {
            let s = &r.signers[i];
            let aperta = aperte.contains(&i) || r.signers.len() == 1;
            let rientro = if s.counter_of.is_some() { 18.0 } else { 0.0 };
            let (sx, sw) = (x + rientro, w - rientro);
            let c = colore_livello(self, s.level);
            let quando = s.when.map(|t| format!("{}, {}", firma::long_date(t), firma::hm(t))).unwrap_or_default();
            let mut sotto = [quando, s.role.clone()].into_iter().filter(|t| !t.is_empty()).collect::<Vec<_>>().join(" · ");
            if let Some(n) = s.place.as_ref().filter(|pl| pl.visible).and_then(|pl| pl.page) {
                if !sotto.contains("pagina") {
                    sotto.push_str(&format!(" · firma visibile a pagina {n}"));
                }
            }
            let tw = sw - 112.0;
            let mh = p.wrap_height(&sotto, 12.5, Weight::Regular, tw, 17.0, 3).max(17.0);
            let hh = 64.0 + mh;
            // La scheda: testa sempre, corpo se aperta.
            let corpo_h = if aperta { self.corpo_firma_h(p, r, i, sw - 28.0, tecnici.contains(&i)) } else { 0.0 };
            super::sollevato(p, pal, scuro, sx, y, sw, hh + corpo_h, 12.0);
            let a = Azione::Firma(FirmaAz::Firma(i));
            if self.premuto == Some(a) {
                p.rrect(sx, y, sw, hh, 12.0, Some(pal.alt), None);
            }
            avatar(p, self, sx + 34.0, y + hh / 2.0, 22.0, &s.initials, s.level);
            p.text(&s.name, sx + 68.0, y + 8.0, tw, 24.0, 15.5, Weight::Bold, pal.text, Align::Left);
            p.text_wrap(&sotto, sx + 68.0, y + 32.0, tw, 12.5, Weight::Regular, pal.muted, 17.0, 3);
            p.text(&s.summary, sx + 68.0, y + 34.0 + mh, tw, 22.0, 14.0, Weight::Semibold, c, Align::Left);
            p.icon(if aperta { "M6 15l6-6 6 6" } else { ic::CHEVRON_DOWN }, sx + sw - 34.0, y + hh / 2.0 - 11.0, 22.0, pal.muted);
            self.colpo((sx, y, sw, hh), a, vista);
            let mut cy = y + hh;
            if aperta {
                cy = self.disegna_corpo_firma(p, r, i, sx + 14.0, cy, sw - 28.0, tecnici.contains(&i), revisione, vista);
            }
            y = cy.max(y + hh) + 12.0;
        }
        if !r.timestamps.is_empty() {
            y += 4.0;
            p.text(if r.timestamps.len() > 1 { "Marche temporali" } else { "Marca temporale" }, x, y, w, 26.0, 16.0, Weight::Semibold, pal.text, Align::Left);
            y += 32.0;
            for t in &r.timestamps {
                let c = colore_livello(self, t.level);
                p.icon(ic::CLOCK, x, y + 2.0, 20.0, c);
                p.text(&format!("{}, {}", firma::long_date(t.time), firma::hm(t.time)), x + 30.0, y, w - 30.0, 22.0, 14.5, Weight::Semibold, pal.text, Align::Left);
                let riga = format!("{} · {}", t.tsa, t.text);
                let lh = p.text_wrap(&riga, x + 30.0, y + 24.0, w - 30.0, 12.5, Weight::Regular, if t.level == Level::Ok { pal.muted } else { c }, 17.0, 4);
                y += 32.0 + lh;
            }
            y += 8.0;
        }
        y
    }

    /// Quanto è alto l'interno di una firma aperta (per disegnare prima la scheda sotto).
    fn corpo_firma_h(&self, p: &Painter, r: &Report, i: usize, w: f32, tecnici: bool) -> f32 {
        let s = &r.signers[i];
        let mut h = p.wrap_height(&s.why, 13.5, Weight::Regular, w - 24.0, 19.0, 30) + 20.0 + 14.0;
        let cw = (w - 12.0) / 2.0;
        for cks in s.checks.chunks(2) {
            let rh = cks.iter().map(|ck| 21.0 + p.wrap_height(&ck.text, 12.0, Weight::Regular, cw, 16.0, 4).max(16.0)).fold(0.0, f32::max);
            h += rh + 10.0;
        }
        if s.revision_end.is_some() {
            h += 50.0;
        }
        h += 40.0;
        if tecnici {
            for (_, val) in &s.details {
                h += 18.0 + p.wrap_height(val, 13.0, Weight::Regular, w, 18.0, 4).max(18.0) + 8.0;
            }
            h += 8.0 + 34.0 * (s.chain.len() as f32 / 2.0).ceil();
        }
        h + 6.0
    }

    #[allow(clippy::too_many_arguments)]
    fn disegna_corpo_firma(&mut self, p: &mut Painter, r: &Report, i: usize, x: f32, mut y: f32, w: f32, tecnici: bool, revisione: Option<usize>, vista: R) -> f32 {
        let pal = self.pal();
        let s = &r.signers[i];
        let c = colore_livello(self, s.level);
        // Il perché.
        let th = p.wrap_height(&s.why, 13.5, Weight::Regular, w - 24.0, 19.0, 30);
        p.rrect(x, y, w, th + 20.0, 8.0, Some(mix(c, pal.surface, 0.9)), None);
        p.fill(x, y + 6.0, 3.0, th + 8.0, c);
        p.text_wrap(&s.why, x + 12.0, y + 10.0, w - 24.0, 13.5, Weight::Regular, pal.text, 19.0, 30);
        y += th + 34.0;
        // I quattro controlli, due per riga.
        let cw = (w - 12.0) / 2.0;
        for cks in s.checks.chunks(2) {
            let mut rh: f32 = 0.0;
            for (k, ck) in cks.iter().enumerate() {
                let cx = x + k as f32 * (cw + 12.0);
                let cc = if ck.counts || ck.level == Level::Ok { colore_livello(self, ck.level) } else { pal.muted };
                p.icon(segno(ck.level), cx - 1.0, y + 3.0, 15.0, cc);
                p.text(ck.what, cx + 18.0, y, cw - 18.0, 20.0, 13.5, Weight::Bold, cc, Align::Left);
                let th = p.text_wrap(&ck.text, cx, y + 21.0, cw, 12.0, Weight::Regular, pal.muted, 16.0, 4).max(16.0);
                rh = rh.max(21.0 + th);
            }
            y += rh + 10.0;
        }
        if s.revision_end.is_some() {
            let vista_ora = revisione == Some(i);
            let t = if vista_ora { "Torna al file com'è adesso" } else { "Vedi la versione firmata" };
            let b = (x, y, p.measure(t, 14.0, Weight::Semibold) + 32.0, 40.0);
            let a = Azione::Firma(if vista_ora { FirmaAz::Torna } else { FirmaAz::Versione(i) });
            p.rrect(b.0, b.1, b.2, b.3, 8.0, Some(if self.premuto == Some(a) { pal.alt } else { pal.surface }), Some((pal.border, 1.0)));
            p.text(t, b.0, b.1, b.2, b.3, 14.0, Weight::Semibold, pal.text, Align::Center);
            self.colpo(b, a, vista);
            y += 50.0;
        }
        // Dettagli tecnici.
        let a = Azione::Firma(FirmaAz::Tecnici(i));
        p.icon(if tecnici { ic::CHEVRON_DOWN } else { "M9 6l6 6-6 6" }, x - 2.0, y + 9.0, 18.0, pal.primary);
        p.text("Dettagli tecnici del certificato", x + 20.0, y, w - 20.0, 36.0, 14.0, Weight::Semibold, pal.primary, Align::Left);
        self.colpo((x - 8.0, y, w + 16.0, 36.0), a, vista);
        y += 40.0;
        if tecnici {
            for (k, val) in &s.details {
                p.text(k, x, y, w, 18.0, 12.0, Weight::Regular, pal.muted, Align::Left);
                y += 18.0;
                y += p.text_wrap(val, x, y, w, 13.0, Weight::Regular, pal.text, 18.0, 4).max(18.0) + 8.0;
            }
            // La catena: elenco UE › certificatore › persona.
            let mut cx = x;
            let mut cy = y + 4.0;
            for (k, passo) in s.chain.iter().enumerate() {
                let tw = (p.measure(passo, 12.0, Weight::Semibold) + 18.0).min(w);
                if k > 0 {
                    if cx + 16.0 + tw > x + w {
                        cx = x;
                        cy += 34.0;
                    } else {
                        p.text("›", cx + 2.0, cy, 14.0, 26.0, 14.0, Weight::Regular, pal.muted, Align::Left);
                        cx += 16.0;
                    }
                }
                p.rrect(cx, cy, tw, 26.0, 13.0, Some(pal.alt), None);
                p.text(passo, cx + 9.0, cy, tw - 18.0, 26.0, 12.0, Weight::Semibold, pal.text, Align::Left);
                cx += tw + 4.0;
            }
            y = cy + 38.0;
        }
        y + 6.0
    }

    /// Verifica alla data: oggi o quando è stato firmato.
    fn alla_data(&mut self, p: &mut Painter, at: At, x: f32, mut y: f32, w: f32, vista: R) -> f32 {
        let pal = self.pal();
        y += 6.0;
        p.text("Verifica alla data", x, y, w, 26.0, 16.0, Weight::Semibold, pal.text, Align::Left);
        y += 34.0;
        let sel = if matches!(at, At::Signing) { 1 } else { 0 };
        p.rrect(x, y, w, 44.0, 10.0, Some(pal.surface), Some((pal.border, 1.0)));
        let mezzo = w / 2.0;
        // La pastiglia scivola sulla scelta.
        let px = self.anim.verso(super::anim::Chiave::Interruttore(2), x + sel as f32 * mezzo, 200.0);
        p.rrect(px + 3.0, y + 3.0, mezzo - 6.0, 38.0, 8.0, Some(pal.primary), None);
        for (k, t) in ["Oggi", "Quando è stato firmato"].iter().enumerate() {
            let rc = (x + k as f32 * mezzo, y, mezzo, 44.0);
            p.text(t, rc.0, rc.1, rc.2, rc.3, 14.0, Weight::Semibold, if k == sel { pal.on_primary } else { pal.text }, Align::Center);
            self.colpo(rc, Azione::Firma(FirmaAz::Alla(k as u8)), vista);
        }
        y += 54.0;
        let nota = if sel == 1 {
            "Si controlla all'ora della marca o, senza marca, a quella scritta da chi ha firmato (non è certa)."
        } else {
            "Le firme con la marca temporale si controllano all'ora della marca; le altre a oggi."
        };
        y += p.text_wrap(nota, x, y, w, 13.0, Weight::Regular, pal.muted, 18.0, 3) + 16.0;
        y
    }
}

const I_CONDIVIDI: &str = "M6 9.5a2.5 2.5 0 1 0 0 5a2.5 2.5 0 1 0 0-5z M18 3.5a2.5 2.5 0 1 0 0 5a2.5 2.5 0 1 0 0-5z M18 15.5a2.5 2.5 0 1 0 0 5a2.5 2.5 0 1 0 0-5z M8.2 11l7.6-4 M8.2 13l7.6 4";

/// Il file firmato è un PDF: il documento è il file stesso.
fn proprio_pdf(p: &Path) -> bool {
    p.extension().is_some_and(|e| e.eq_ignore_ascii_case("pdf"))
}

fn avatar(p: &mut Painter, app: &App, cx: f32, cy: f32, rag: f32, iniziali: &str, l: Level) {
    let pal = app.pal();
    let c = colore_livello(app, l);
    p.circle(cx, cy, rag, Some(mix(c, pal.surface, 0.82)), None);
    p.text(iniziali, cx - rag, cy - rag, rag * 2.0, rag * 2.0, rag * 0.72, Weight::Bold, mix(c, pal.text, 0.35), Align::Center);
    let mr = rag * 0.42;
    let (mx, my) = (cx + rag * 0.72, cy + rag * 0.72);
    p.circle(mx, my, mr + 2.0, Some(pal.surface), None);
    p.circle(mx, my, mr, Some(c), None);
    p.icon(segno(l), mx - mr * 0.75, my - mr * 0.75, mr * 1.5, (255, 255, 255));
}

/// Un interruttore a `t` (0 spento, 1 acceso), con il pallino che scorre.
pub(super) fn interruttore(p: &mut Painter, pal: &cedro::ui::theme::Palette, x: f32, cy: f32, t: f32) {
    let fondo = mix(pal.border, pal.primary, t);
    p.rrect(x, cy - 14.0, 46.0, 28.0, 14.0, Some(fondo), None);
    p.circle(x + 14.0 + 18.0 * t, cy, 10.0, Some(mix(pal.surface, pal.on_primary, t)), None);
}

/// La rotella di attesa (gira finché si ridisegna).
fn rotella(p: &mut Painter, app: &App, cx: f32, cy: f32, c: Rgb) {
    let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| (d.as_millis() % 100_000) as f32 / 1000.0).unwrap_or(0.0);
    let a0 = (t * 6.0) % std::f32::consts::TAU;
    for k in 0..8 {
        let a = a0 + k as f32 * std::f32::consts::TAU / 8.0;
        let alfa = 0.25 + 0.75 * (k as f32 / 7.0);
        p.circle(cx + 10.0 * a.cos(), cy + 10.0 * a.sin(), 2.4, Some(mix(app.pal().surface, c, alfa)), None);
    }
}
