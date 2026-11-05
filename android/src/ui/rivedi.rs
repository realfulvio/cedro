//! «Rivedi trascrizione»: le frasi con il tempo, l'ascolto dal punto che si tocca, i difetti tipici di whisper
//! segnalati (ripetizioni, giri di parole, frasi inventate nel silenzio) e il salvataggio in TXT, SRT o VTT.
//! Lo stato e le correzioni sono quelli del computer (`cedro::ui::review::Review`); qui c'è il disegno per il tocco.
//! Come sul computer (v0.30-v0.31) ci sono anche «Chi parla» (nomi, colori, unisci) e il Riassunto AI.

use std::path::Path;
use std::sync::atomic::AtomicU32;
use std::sync::Arc;
use std::time::Instant;

use cedro::revisione::{self, Kind};
use cedro::trascrizione;
use cedro::ui::gfx::{Align, Painter, Weight};
use cedro::ui::player::Player;
use cedro::ui::review::Review;
use cedro::ui::theme::mix;

use super::{adesso, ic, App, Azione, Effetto, Forma, Lavoro, Stato, BARRA, TOCCO};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RivAz {
    Chiudi,
    Play,
    Salta(i32),
    /// Un punto dell'onda (in millesimi della durata).
    Onda(u32),
    SoloControllo,
    Segui,
    /// Il tempo di una frase: si ascolta da lì.
    Riga(usize),
    Sistema(usize),
    Lascia(usize),
    SistemaTutte,
    /// 0 TXT, 1 SRT, 2 VTT.
    Salva(u8),
    /// La scheda: 0 Frasi, 1 Chi parla, 2 Riassunto.
    Scheda(u8),
    /// Il nome colorato di una frase: si sceglie chi la dice.
    Chip(usize),
    /// Nel foglio della frase: questa voce (NEW_VOICE = una voce nuova).
    Voce(u8),
    /// «Chi parla»: ascolta un pezzo della voce.
    Ascolta(u8),
    /// «Chi parla»: unisci questa voce a un'altra (apre il foglio).
    Unisci(u8),
    /// Nel foglio di «Unisci»: la voce in cui finisce.
    UnisciIn(u8),
    /// «Scrivi chi parla nel file».
    ScriviVoci,
    /// Riassunto: in cima al file salvato.
    RiassuntoInCima,
    /// Riassunto: spunta una cosa da fare.
    FattoTodo(usize),
    /// Riassunto: fallo (o rifallo).
    Riassumi,
    ChiudiFoglio,
}

/// Il foglio che sale dal basso nella revisione.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FoglioRiv {
    /// Chi dice la frase (riga).
    Voce(usize),
    /// In quale voce unire questa.
    Unisci(u8),
}

pub struct Rivedi {
    pub r: Review,
    pub titolo: String,
    player: Option<Player>,
    /// Il lavoro che sta salvando, e cosa dire sotto.
    pub salva: Option<u32>,
    pub nota: String,
    /// Altezze delle frasi alla larghezza `largo` (si rifanno quando cambia qualcosa).
    altezze: Vec<f32>,
    largo: f32,
    scorri: f32,
    scorri_max: f32,
    /// Dove sta l'onda sullo schermo, per toccarla.
    onda: (f32, f32),
    pub foglio: Option<FoglioRiv>,
    /// Il Riassunto AI in corso: il lavoro e a che punto è (0..1000), e cosa sta facendo.
    pub riassunto: Option<(u32, Arc<AtomicU32>)>,
    pub riassunto_nota: String,
}

impl Rivedi {
    pub fn suona(&self) -> bool {
        self.player.as_ref().is_some_and(|p| p.playing() || p.loading())
    }

    fn posizione(&self) -> f64 {
        self.player.as_ref().map_or(0.0, |p| p.position())
    }

    fn lettore(&mut self) -> &mut Player {
        let (src, secs) = (self.r.source.clone(), self.r.secs);
        self.player.get_or_insert_with(|| Player::start(&src, false, Some(secs).filter(|s| *s > 0.0), Arc::new(|| {})))
    }

    fn suona_da(&mut self, secs: f64) {
        let nuovo = self.player.is_none();
        let p = self.lettore();
        p.seek(secs.max(0.0));
        if !nuovo && !p.playing() {
            p.toggle();
        }
    }

    fn rifai(&mut self) {
        self.altezze.clear();
    }

    /// Nomi, voci e riassunto restano nella revisione salvata: la prossima volta si riparte da qui.
    pub(super) fn salva_revisione(&self) {
        let segs = self.r.segments();
        let _ = revisione::save_with(&self.r.source, self.r.secs, &self.r.lang, &segs, &self.r.names, self.r.summary.as_ref());
    }

    /// Un tasto sul nome di una voce («Chi parla»).
    pub(super) fn tasto_nome(&mut self, v: u8, t: super::Tasto) {
        let Some(n) = self.r.names.get_mut(v as usize) else { return };
        match t {
            super::Tasto::Canc => {
                n.pop();
            }
            super::Tasto::Car(c) if !c.is_control() && n.chars().count() < 40 => n.push(c),
            _ => {}
        }
        self.rifai();
    }

    /// Quanti punti si sistemano con «Sistema tutte le ripetizioni».
    fn ripetizioni(&self) -> usize {
        (0..self.r.flags.len()).filter(|i| matches!(self.r.flags[*i].kind, Kind::Repeat | Kind::Loop) && self.r.is_open(*i)).count()
    }
}

/// «16:42» o «1:02:03» da secondi.
fn orologio(secs: f64) -> String {
    let s = secs.max(0.0) as u64;
    if s >= 3600 {
        format!("{}:{:02}:{:02}", s / 3600, s / 60 % 60, s % 60)
    } else {
        format!("{}:{:02}", s / 60, s % 60)
    }
}

fn etichetta(kind: Kind, count: usize) -> String {
    match kind {
        Kind::Repeat => format!("Ripetuta {count} volte"),
        Kind::Loop => "Giro di parole".into(),
        Kind::Typical => "Frase tipica".into(),
        Kind::Silence => "Nel silenzio".into(),
    }
}

/// Quanti punti da controllare ha la trascrizione salvata di `source` (senza l'onda: ripetizioni e frasi tipiche).
pub fn da_controllare(source: &Path) -> Option<usize> {
    let s = revisione::load(source)?;
    Some(Review::new(source, s.secs, s.lang, s.segments, s.names, s.summary).open_count())
}

impl App {
    /// Apre la revisione della trascrizione del lavoro `id`. L'onda la legge `glue` (Effetto::OndaRevisione).
    pub(super) fn apri_rivedi(&mut self, id: u32) -> Effetto {
        let Some(src) = self.lavori.iter().find(|l| l.id == id).and_then(|l| l.sorgente.clone()) else { return Effetto::Niente };
        let Some(s) = revisione::load(&src) else { return Effetto::Niente };
        let titolo = self.lavori.iter().find(|l| l.id == id).map(|l| l.origine.clone()).unwrap_or_default();
        let titolo = Path::new(&titolo).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or(titolo);
        let secs = s.secs;
        self.rivedi = Some(Rivedi { r: Review::new(&src, secs, s.lang, s.segments, s.names, s.summary), titolo, player: None, salva: None, nota: String::new(), altezze: Vec::new(), largo: 0.0, scorri: 0.0, scorri_max: 0.0, onda: (0.0, 0.0), foglio: None, riassunto: None, riassunto_nota: String::new() });
        Effetto::OndaRevisione(src, secs)
    }

    /// Apre una revisione già pronta (per le fotografie dell'esempio `vista`).
    pub fn apri_rivedi_con(&mut self, r: Review, titolo: String, scheda: u8, foglio: Option<FoglioRiv>, riassunto: Option<(u32, u32, String)>) {
        let mut rv = Rivedi { r, titolo, player: None, salva: None, nota: String::new(), altezze: Vec::new(), largo: 0.0, scorri: 0.0, scorri_max: 0.0, onda: (0.0, 0.0), foglio, riassunto: None, riassunto_nota: String::new() };
        rv.r.tab = scheda;
        if let Some((id, m, nota)) = riassunto {
            rv.riassunto = Some((id, Arc::new(AtomicU32::new(m))));
            rv.riassunto_nota = nota;
        }
        self.rivedi = Some(rv);
    }

    /// L'onda dell'audio è pronta: con lei si trovano anche le frasi nel silenzio.
    pub fn onda_rivedi(&mut self, path: &Path, bars: Option<Vec<f32>>) {
        let Some(rv) = self.rivedi.as_mut().filter(|rv| rv.r.source == path) else { return };
        let r = &mut rv.r;
        r.bars = bars;
        if r.bars.is_some() {
            let prima: Vec<(Kind, Vec<usize>)> = r.dismissed.iter().filter_map(|i| r.flags.get(*i).map(|f| (f.kind, f.segs.clone()))).collect();
            r.detect();
            r.dismissed = r.flags.iter().enumerate().filter(|(_, f)| prima.iter().any(|(k, s)| *k == f.kind && *s == f.segs)).map(|(i, _)| i).collect();
        }
        rv.rifai();
    }

    /// «Fai il riassunto»: parte come un lavoro (con la notifica), il risultato torna qui.
    pub(super) fn avvia_riassunto(&mut self) -> Effetto {
        let Some(rv) = self.rivedi.as_mut() else { return Effetto::Niente };
        if rv.riassunto.is_some() {
            return Effetto::Niente;
        }
        let names = rv.r.current_names();
        let lines = cedro::riassunto::lines_for(&rv.r.segments(), &names);
        if lines.is_empty() {
            rv.riassunto_nota = "La trascrizione è vuota: non c'è niente da riassumere.".into();
            return Effetto::Ridisegna;
        }
        let id = self.prossimo;
        self.prossimo += 1;
        let prog = Arc::new(AtomicU32::new(u32::MAX));
        let rv = self.rivedi.as_mut().expect("aperta");
        rv.riassunto = Some((id, prog.clone()));
        rv.riassunto_nota = "Accendo il motore…".into();
        let source = rv.r.source.clone();
        let origine = source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        self.lavori.insert(0, Lavoro { id, origine, etichetta: "Riassunto AI".into(), conv: false, inizio: Instant::now(), quando: adesso(), stato: Stato::InCorso, prog: prog.clone(), sorgente: None, controlli: None });
        Effetto::Riassumi { id, source, lines, names, prog }
    }

    /// A che punto è il riassunto (il passo, in parole).
    pub fn passo_riassunto(&mut self, id: u32, nota: String) {
        if self.lavori.iter().any(|l| l.id == id) {
            self.passi.insert(id, nota.clone());
        }
        if let Some(rv) = self.rivedi.as_mut().filter(|rv| rv.riassunto.as_ref().is_some_and(|r| r.0 == id)) {
            rv.riassunto_nota = nota;
        }
    }

    /// Il riassunto è pronto: va nella revisione aperta (se è quella) e resta salvato con la trascrizione.
    pub fn riassunto_pronto(&mut self, source: &Path, s: cedro::riassunto::Stored) {
        match self.rivedi.as_mut().filter(|rv| rv.r.source == source) {
            Some(rv) => {
                rv.r.summary = Some(s);
                rv.riassunto_nota.clear();
                rv.scorri = 0.0;
                rv.salva_revisione();
            }
            None => {
                if let Some(sv) = revisione::load(source) {
                    let _ = revisione::save_with(source, sv.secs, &sv.lang, &sv.segments, &sv.names, Some(&s));
                }
            }
        }
    }

    /// Un salvataggio della revisione è finito.
    pub(super) fn salvata(&mut self, id: u32, stato: &Stato) {
        if let Some(rv) = self.rivedi.as_mut().filter(|rv| rv.salva == Some(id)) {
            rv.salva = None;
            rv.nota = match stato {
                Stato::Fatto { nome, .. } => super::salvato_in(nome),
                Stato::Errore(e) => super::dividi_errore(e).0.to_string(),
                Stato::InCorso => String::new(),
            };
        }
    }

    pub(super) fn azione_rivedi(&mut self, az: RivAz) -> Effetto {
        let Some(rv) = self.rivedi.as_mut() else { return Effetto::Niente };
        match az {
            RivAz::Chiudi => {
                // La scheda della trascrizione dice quanti punti restano da controllare.
                let (src, n) = (rv.r.source.clone(), rv.r.open_count());
                self.rivedi = None;
                for l in self.lavori.iter_mut().filter(|l| l.sorgente.as_ref() == Some(&src)) {
                    l.controlli = Some(n);
                }
                return Effetto::SalvaLavori;
            }
            RivAz::Play => {
                let nuovo = rv.player.is_none();
                let da = rv.r.sel.and_then(|i| rv.r.rows.get(i)).map(|row| row.t0 as f64 / 100.0);
                let p = rv.lettore();
                if nuovo {
                    if let Some(s) = da {
                        p.seek(s);
                    }
                } else {
                    p.toggle();
                }
            }
            RivAz::Salta(d) => {
                let max = (rv.r.secs - 0.1).max(0.0);
                let to = (rv.posizione() + d as f64).clamp(0.0, max);
                rv.suona_da(to);
            }
            RivAz::Onda(m) => {
                let to = rv.r.secs * m as f64 / 1000.0;
                rv.suona_da(to);
            }
            RivAz::SoloControllo => {
                rv.r.only_flagged = !rv.r.only_flagged;
                rv.scorri = 0.0;
                rv.rifai();
            }
            RivAz::Segui => rv.r.follow = !rv.r.follow,
            RivAz::Riga(i) => {
                if let Some(t0) = rv.r.rows.get(i).map(|x| x.t0) {
                    rv.r.sel = Some(i);
                    rv.r.followed = Some(i);
                    rv.suona_da(t0 as f64 / 100.0);
                }
            }
            RivAz::Sistema(f) => {
                rv.r.push_undo();
                rv.r.fix(f);
                rv.rifai();
            }
            RivAz::Lascia(f) => {
                rv.r.dismissed.insert(f);
                rv.rifai();
            }
            RivAz::SistemaTutte => {
                rv.r.push_undo();
                rv.r.fix_repeats();
                rv.rifai();
            }
            RivAz::Salva(k) => {
                if rv.salva.is_some() {
                    return Effetto::Niente;
                }
                let segs = rv.r.segments();
                // Con l'interruttore spento il file esce senza chi parla (i nomi restano nella revisione).
                let nomi: &[String] = if rv.r.write_voices { &rv.r.names } else { &[] };
                // Il riassunto va solo nel testo: i sottotitoli restano come sono.
                let riassunto = rv.r.summary_sections().filter(|_| rv.r.summary_on_top);
                let (ext, dati) = match k {
                    1 => ("srt", revisione::srt_bytes(&segs, nomi)),
                    2 => ("vtt", revisione::vtt_bytes(&segs, nomi)),
                    _ => ("txt", trascrizione::text_bytes_with(&rv.r.source, rv.r.secs, &rv.r.lang, &segs, nomi, riassunto.as_ref())),
                };
                // Le correzioni (e il riassunto) restano: la prossima volta si riparte da qui.
                let _ = revisione::save_with(&rv.r.source, rv.r.secs, &rv.r.lang, &segs, &rv.r.names, rv.r.summary.as_ref());
                let nome = format!("{} - rivisto.{ext}", rv.titolo);
                let id = self.prossimo;
                self.prossimo += 1;
                let rv = self.rivedi.as_mut().expect("aperta");
                rv.salva = Some(id);
                rv.nota = "Salvo…".into();
                let origine = rv.r.source.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                let et = format!("Rivisto {}", ext.to_uppercase());
                self.lavori.insert(0, Lavoro { id, origine, etichetta: et, conv: true, inizio: Instant::now(), quando: adesso(), stato: Stato::InCorso, prog: Arc::new(AtomicU32::new(u32::MAX)), sorgente: None, controlli: None });
                return Effetto::Scrivi { id, nome, dati };
            }
            RivAz::Scheda(t) => {
                rv.r.tab = t;
                rv.scorri = 0.0;
                rv.foglio = None;
            }
            RivAz::Chip(i) => rv.foglio = Some(FoglioRiv::Voce(i)),
            RivAz::Voce(v) => {
                if let Some(FoglioRiv::Voce(i)) = rv.foglio.take() {
                    rv.r.push_undo();
                    rv.r.set_speaker(i, v);
                    rv.rifai();
                    rv.salva_revisione();
                }
            }
            RivAz::Ascolta(v) => {
                if let Some(t0) = rv.r.voice_sample(v).and_then(|i| rv.r.rows.get(i)).map(|r| r.t0) {
                    rv.suona_da(t0 as f64 / 100.0);
                }
            }
            RivAz::Unisci(v) => rv.foglio = Some(FoglioRiv::Unisci(v)),
            RivAz::UnisciIn(into) => {
                if let Some(FoglioRiv::Unisci(from)) = rv.foglio.take() {
                    rv.r.push_undo();
                    rv.r.merge(from, into);
                    rv.rifai();
                    rv.salva_revisione();
                }
            }
            RivAz::ScriviVoci => rv.r.write_voices = !rv.r.write_voices,
            RivAz::RiassuntoInCima => rv.r.summary_on_top = !rv.r.summary_on_top,
            RivAz::FattoTodo(k) => {
                if let Some(s) = rv.r.summary.as_mut() {
                    match s.done.iter().position(|d| *d == k) {
                        Some(i) => {
                            s.done.remove(i);
                        }
                        None => s.done.push(k),
                    }
                }
                rv.salva_revisione();
            }
            RivAz::Riassumi => return self.avvia_riassunto(),
            RivAz::ChiudiFoglio => rv.foglio = None,
        }
        Effetto::Ridisegna
    }

    // ---- Disegno ----

    pub(super) fn pagina_rivedi(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        let avviso = self.avviso();
        let premuto = self.premuto;
        let mut colpi: Vec<(Forma, Azione)> = Vec::new();
        let Some(rv) = self.rivedi.as_mut() else { return };
        // Barra
        let indietro = (4.0, 4.0, TOCCO, TOCCO);
        if premuto == Some(Azione::Riv(RivAz::Chiudi)) {
            p.rrect_a(indietro.0, indietro.1, indietro.2, indietro.3, 24.0, pal.primary, 0.14);
        }
        p.icon(ic::ARROW_LEFT, 15.0, 15.0, 26.0, pal.text);
        colpi.push((Forma::R(indietro), Azione::Riv(RivAz::Chiudi)));
        p.text(&rv.titolo, 60.0, 0.0, w - 76.0, BARRA, 21.0, Weight::Semibold, pal.text, Align::Left);
        let ampio = w > 700.0;
        let cw = if ampio { 620.0 } else { w - 24.0 };
        let x = if ampio { (w - cw) / 2.0 } else { 12.0 };
        let mut y = BARRA + 2.0;

        // Il lettore: onda, salti, play e tempo.
        let card_h = 126.0;
        super::sollevato(p, pal, self.tema.dark(), x, y, cw, card_h, 20.0);
        let pos = rv.posizione();
        let durata = rv.r.secs.max(0.001);
        let (ox, ow, oy, oh) = (x + 14.0, cw - 28.0, y + 12.0, 46.0);
        rv.onda = (ox, ow);
        match &rv.r.bars {
            Some(bars) if !bars.is_empty() => {
                let n = ((ow / 6.0) as usize).max(10);
                let fatto = (pos / durata) as f32;
                for k in 0..n {
                    let a = k * bars.len() / n;
                    let b = ((k + 1) * bars.len() / n).max(a + 1).min(bars.len());
                    let v = bars[a..b].iter().cloned().fold(0.0f32, f32::max).clamp(0.04, 1.0);
                    let bh = v * oh;
                    let bx = ox + k as f32 * ow / n as f32;
                    let col = if (k as f32 + 0.5) / n as f32 <= fatto { pal.primary } else { pal.border };
                    p.rrect(bx, oy + (oh - bh) / 2.0, (ow / n as f32 - 2.0).max(2.0), bh, 1.5, Some(col), None);
                }
            }
            _ => p.text("Leggo l'onda dell'audio…", ox, oy, ow, oh, 14.0, Weight::Regular, pal.muted, Align::Center),
        }
        // Tocchi sull'onda: dieci zone per saltare (il punto preciso lo dà la zona).
        for k in 0..40u32 {
            colpi.push((Forma::R((ox + k as f32 * ow / 40.0, oy - 6.0, ow / 40.0, oh + 12.0)), Azione::Riv(RivAz::Onda(k * 25 + 12))));
        }
        let cy = y + 92.0;
        let mut bx = x + 14.0;
        for (t, az) in [("−10", RivAz::Salta(-10)), ("", RivAz::Play), ("+10", RivAz::Salta(10))] {
            let a = Azione::Riv(az);
            let r = if az == RivAz::Play { 24.0 } else { 21.0 };
            let c = (bx + r, cy);
            if az == RivAz::Play {
                p.circle(c.0, c.1, r, Some(if premuto == Some(a) { mix(pal.primary, (0, 0, 0), 0.15) } else { pal.primary }), None);
                let suona = rv.player.as_ref().is_some_and(|p| p.playing());
                if suona {
                    p.rrect(c.0 - 7.0, c.1 - 8.0, 5.0, 16.0, 1.0, Some(pal.on_primary), None);
                    p.rrect(c.0 + 2.0, c.1 - 8.0, 5.0, 16.0, 1.0, Some(pal.on_primary), None);
                } else {
                    p.icon("M9 6l9 6-9 6z", c.0 - 11.0, c.1 - 12.0, 24.0, pal.on_primary);
                }
            } else {
                p.circle(c.0, c.1, r, Some(if premuto == Some(a) { pal.alt } else { pal.surface }), Some((pal.border, 2.0)));
                p.text(t, c.0 - r, c.1 - r, 2.0 * r, 2.0 * r, 13.0, Weight::Semibold, pal.text, Align::Center);
            }
            colpi.push((Forma::Cerchio { cx: c.0, cy: c.1, r: r.max(24.0) }, a));
            bx += 2.0 * r + 12.0;
        }
        let tempo = format!("{} / {}", orologio(pos), orologio(rv.r.secs));
        p.text(&tempo, bx, cy - 16.0, x + cw - 14.0 - bx, 32.0, 16.0, Weight::Semibold, pal.text, Align::Right);
        y += card_h + 8.0;

        // Le schede: Frasi, Chi parla (se ci sono le voci), Riassunto.
        let voci = !rv.r.names.is_empty();
        let schede: Vec<(&str, u8)> = if voci { vec![("Frasi", 0), ("Chi parla", 1), ("Riassunto", 2)] } else { vec![("Frasi", 0), ("Riassunto", 2)] };
        if !voci && rv.r.tab == 1 {
            rv.r.tab = 0;
        }
        let sw = cw / schede.len() as f32;
        p.rrect(x, y, cw, 44.0, 22.0, Some(pal.alt), None);
        for (k, (t, id)) in schede.iter().enumerate() {
            let rc = (x + k as f32 * sw + 3.0, y + 3.0, sw - 6.0, 38.0);
            let on = rv.r.tab == *id;
            if on {
                super::sollevato(p, pal, self.tema.dark(), rc.0, rc.1, rc.2, rc.3, 19.0);
            }
            p.text(t, rc.0, rc.1, rc.2, rc.3, 15.0, if on { Weight::Semibold } else { Weight::Regular }, if on { pal.text } else { pal.muted }, Align::Center);
            colpi.push((Forma::R((rc.0, y, rc.2, 44.0)), Azione::Riv(RivAz::Scheda(*id))));
        }
        y += 52.0;
        if rv.r.tab != 0 {
            let tema = self.tema;
            let campo = self.campo;
            if rv.r.tab == 1 {
                chi_parla(p, pal, tema, rv, (x, y, cw, h - y), campo, premuto, &mut colpi);
            } else {
                riassunto(p, pal, tema, rv, (x, y, cw, h - y), premuto, &mut colpi);
            }
            self.colpi.extend(colpi);
            self.foglio_rivedi(p, w, h);
            return;
        }

        // Solo da controllare, Segui l'audio
        let mezza = cw / 2.0;
        for (k, (t, on, az)) in [("Solo da controllare", rv.r.only_flagged, RivAz::SoloControllo), ("Segui l'audio", rv.r.follow, RivAz::Segui)].into_iter().enumerate() {
            let rx = x + k as f32 * mezza;
            let dim = if p.measure(t, 14.0, Weight::Regular) <= mezza - 70.0 { 14.0 } else { 12.0 };
            p.text(t, rx + 4.0, y, mezza - 66.0, 40.0, dim, Weight::Regular, pal.text, Align::Left);
            let sx = rx + mezza - 62.0;
            p.rrect(sx, y + 6.0, 48.0, 28.0, 14.0, Some(if on { pal.primary } else { pal.border }), None);
            p.circle(if on { sx + 34.0 } else { sx + 14.0 }, y + 20.0, 10.0, Some((255, 255, 255)), None);
            colpi.push((Forma::R((rx, y, mezza, 40.0)), Azione::Riv(az)));
        }
        y += 46.0;

        // Le frasi.
        let piede = 132.0 + if rv.nota.is_empty() { 0.0 } else { 26.0 };
        let vista = (0.0, y, w, h - y - piede);
        let visibili = rv.r.visible();
        let tw = cw - 76.0;
        if rv.largo != tw || rv.altezze.len() != visibili.len() {
            rv.largo = tw;
            rv.altezze = visibili
                .iter()
                .map(|i| {
                    let row = &rv.r.rows[*i];
                    let righe = p.wrap_lines(&row.text, 15.0, Weight::Regular, tw, 0).len().max(1) as f32;
                    let aperto = rv.r.flag_of(*i).is_some_and(|f| rv.r.is_open(f) && rv.r.flags[f].segs.first() == Some(i));
                    16.0 + righe * 22.0 + if aperto { 50.0 } else { 0.0 } + if voci { 30.0 } else { 0.0 }
                })
                .collect();
        }
        // «Segui l'audio»: la frase che suona sale in vista.
        let suona = rv.player.as_ref().is_some_and(|p| p.playing());
        if suona && rv.r.follow {
            if let Some(row) = rv.r.row_at(pos) {
                if rv.r.followed != Some(row) {
                    rv.r.followed = Some(row);
                    if let Some(at) = visibili.iter().position(|i| *i == row) {
                        rv.scorri = rv.altezze[..at].iter().map(|a| a + 6.0).sum::<f32>() - 60.0;
                    }
                }
            }
        }
        let totale: f32 = rv.altezze.iter().map(|a| a + 6.0).sum();
        rv.scorri_max = (totale - vista.3 + 8.0).max(0.0);
        rv.scorri = rv.scorri.clamp(0.0, rv.scorri_max);
        p.push_clip(vista.0, vista.1, vista.2, vista.3);
        let corrente = if rv.player.is_some() { rv.r.row_at(pos) } else { rv.r.sel };
        let mut ry = y - rv.scorri;
        if visibili.is_empty() {
            let msg = if rv.r.only_flagged { "Niente da controllare: whisper qui non ha lasciato ripetizioni né frasi inventate." } else { "Nessuna frase da mostrare." };
            p.text_wrap(msg, x + 8.0, y + 12.0, cw - 16.0, 15.0, Weight::Regular, pal.muted, 22.0, 4);
        }
        for (k, i) in visibili.iter().copied().enumerate() {
            let ah = rv.altezze[k];
            if ry + ah < vista.1 {
                ry += ah + 6.0;
                continue;
            }
            if ry > vista.1 + vista.3 {
                break;
            }
            let row = rv.r.rows[i].clone();
            let flag = rv.r.flag_of(i);
            let aperto = flag.is_some_and(|f| rv.r.is_open(f));
            let fondo = if corrente == Some(i) { Some(mix(pal.surface, pal.primary, 0.12)) } else if aperto { Some(mix(pal.surface, avviso, 0.08)) } else { None };
            if let Some(f) = fondo {
                p.rrect(x, ry, cw, ah, 14.0, Some(f), None);
            }
            let rc = (x, ry, 64.0, 38.0);
            p.text(&orologio(row.t0 as f64 / 100.0), rc.0 + 6.0, ry + 6.0, 58.0, 24.0, 13.0, Weight::Semibold, pal.primary, Align::Left);
            colpi.push((Forma::R((rc.0, rc.1, 70.0, ah.max(TOCCO))), Azione::Riv(RivAz::Riga(i))));
            let col = if row.removed { pal.muted } else { pal.text };
            // Chi la dice: il nome colorato; toccandolo si sceglie un'altra voce.
            let ty = if voci {
                let v = row.speaker.unwrap_or(0);
                let vc = cedro::ui::theme::voice(self.tema, v as usize);
                let nome = row.speaker.map(|v| rv.r.name(v)).unwrap_or_else(|| "Chi parla?".into());
                let nw = p.measure(&nome, 13.0, Weight::Semibold).min(tw - 40.0) + 34.0;
                p.rrect(x + 70.0, ry + 6.0, nw, 26.0, 13.0, Some(mix(vc, pal.surface, if self.tema.dark() { 0.78 } else { 0.86 })), None);
                p.circle(x + 82.0, ry + 19.0, 4.5, Some(vc), None);
                p.text(&nome, x + 92.0, ry + 6.0, nw - 30.0, 26.0, 13.0, Weight::Semibold, vc, Align::Left);
                p.icon("M6 9l6 6 6-6", x + 70.0 + nw - 18.0, ry + 12.0, 13.0, vc);
                colpi.push((Forma::R((x + 66.0, ry, nw + 8.0, 38.0)), Azione::Riv(RivAz::Chip(i))));
                ry + 38.0
            } else {
                ry + 8.0
            };
            let usata = p.text_wrap(&row.text, x + 70.0, ty, tw, 15.0, Weight::Regular, col, 22.0, 0);
            if row.removed {
                for l in 0..(usata / 22.0).round().max(1.0) as usize {
                    p.fill(x + 70.0, ty + l as f32 * (usata / (usata / 22.0).round().max(1.0)) + 12.0, tw.min(p.measure(&row.text, 15.0, Weight::Regular)), 1.5, pal.muted);
                }
            }
            if let (Some(f), true) = (flag, aperto && rv.r.flags.get(flag.unwrap_or(0)).and_then(|fl| fl.segs.first()) == Some(&i)) {
                let fl = rv.r.flags[f].clone();
                let by = ry + ah - 46.0;
                let et = etichetta(fl.kind, fl.count);
                let ew = p.measure(&et, 12.0, Weight::Semibold) + 20.0;
                p.rrect(x + 70.0, by + 6.0, ew, 28.0, 14.0, Some(pal.alt), None);
                p.text(&et, x + 70.0, by + 6.0, ew, 28.0, 12.0, Weight::Semibold, avviso, Align::Center);
                let mut bx = x + 70.0 + ew + 8.0;
                let verbo = if matches!(fl.kind, Kind::Repeat | Kind::Loop) { "Sistema" } else { "Togli" };
                for (t, az, pieno) in [(verbo, RivAz::Sistema(f), true), ("Lascia così", RivAz::Lascia(f), false)] {
                    let bw = p.measure(t, 13.0, Weight::Semibold) + 24.0;
                    if bx + bw > x + cw {
                        break;
                    }
                    let a = Azione::Riv(az);
                    let b = (bx, by + 4.0, bw, 32.0);
                    p.rrect(b.0, b.1, b.2, b.3, 16.0, Some(if pieno { pal.primary } else { pal.surface }), if pieno { None } else { Some((pal.border, 2.0)) });
                    p.text(t, b.0, b.1, b.2, b.3, 13.0, Weight::Semibold, if pieno { pal.on_primary } else { pal.text }, Align::Center);
                    colpi.push((Forma::R((b.0, b.1 - 6.0, b.2, b.3 + 12.0)), a));
                    bx += bw + 8.0;
                }
            }
            ry += ah + 6.0;
        }
        p.pop_clip();

        // Il piede: Sistema tutte le ripetizioni, e i tre formati.
        let mut fy = h - piede + 8.0;
        if !rv.nota.is_empty() {
            p.text(&rv.nota, x, fy, cw, 22.0, 13.0, Weight::Regular, pal.muted, Align::Center);
            fy += 26.0;
        }
        let n = rv.ripetizioni();
        let bt = (x, fy, cw, 52.0);
        let a = Azione::Riv(RivAz::SistemaTutte);
        let attivo = n > 0;
        p.rrect(bt.0, bt.1, bt.2, bt.3, 26.0, Some(if !attivo { pal.alt } else if premuto == Some(a) { mix(pal.primary, (0, 0, 0), 0.15) } else { pal.primary }), None);
        let fg = if attivo { pal.on_primary } else { pal.muted };
        let testo = "Sistema tutte le ripetizioni";
        let tw2 = p.measure(testo, 17.0, Weight::Semibold).min(bt.2 - 60.0);
        p.icon(ic::CHECK, bt.0 + (bt.2 - tw2 - 30.0) / 2.0, bt.1 + 14.0, 22.0, fg);
        p.text(testo, bt.0 + (bt.2 - tw2 - 30.0) / 2.0 + 30.0, bt.1, tw2 + 4.0, bt.3, 17.0, Weight::Semibold, fg, Align::Left);
        if attivo {
            colpi.push((Forma::R(bt), a));
        }
        fy += 60.0;
        let sw = (cw - 16.0) / 3.0;
        for (k, t) in ["Salva TXT", "SRT", "VTT"].into_iter().enumerate() {
            let b = (x + k as f32 * (sw + 8.0), fy, sw, 48.0);
            let a = Azione::Riv(RivAz::Salva(k as u8));
            p.rrect(b.0, b.1, b.2, b.3, 24.0, Some(if premuto == Some(a) { pal.alt } else { pal.surface }), Some((pal.primary, 2.0)));
            p.text(t, b.0, b.1, b.2, b.3, 15.0, Weight::Semibold, pal.primary, Align::Center);
            colpi.push((Forma::R(b), a));
        }
        self.colpi.extend(colpi);
        self.foglio_rivedi(p, w, h);
    }

    /// Il foglio che sale dal basso: chi dice una frase, oppure in quale voce unirne un'altra.
    fn foglio_rivedi(&mut self, p: &mut Painter, w: f32, h: f32) {
        let pal = self.pal();
        let tema = self.tema;
        let (ml, mt, mr, mb) = self.margini;
        let Some(rv) = self.rivedi.as_ref() else { return };
        let Some(f) = rv.foglio else { return };
        let (titolo, voci): (String, Vec<(String, u8)>) = match f {
            FoglioRiv::Voce(_) => {
                let mut v: Vec<(String, u8)> = rv.r.live_voices().into_iter().map(|v| (rv.r.name(v), v)).collect();
                v.push(("Voce nuova".into(), cedro::ui::core::NEW_VOICE));
                ("Chi dice questa frase?".into(), v)
            }
            FoglioRiv::Unisci(da) => (format!("Unisci {} a…", rv.r.name(da)), rv.r.live_voices().into_iter().filter(|v| *v != da).map(|v| (rv.r.name(v), v)).collect()),
        };
        p.rrect_a(-ml, -mt, w + ml + mr, h + mt + mb, 0.0, (10, 8, 16), 0.45);
        self.colpi.push((Forma::R((-ml, -mt, w + ml + mr, h + mt + mb)), Azione::Riv(RivAz::ChiudiFoglio)));
        let rh = 54.0;
        let fh = (70.0 + voci.len() as f32 * rh).min(h * 0.8);
        let fy = h - fh;
        p.rrect(0.0, fy, w, fh + mb + 30.0, 24.0, Some(pal.surface), None);
        self.colpi.push((Forma::R((0.0, fy, w, fh)), Azione::Niente));
        p.rrect(w / 2.0 - 20.0, fy + 8.0, 40.0, 5.0, 3.0, Some(pal.border), None);
        p.text(&titolo, 20.0, fy + 18.0, w - 40.0, 34.0, 18.0, Weight::Bold, pal.text, Align::Left);
        for (k, (nome, v)) in voci.iter().enumerate() {
            let ry = fy + 60.0 + k as f32 * rh;
            let nuova = *v == cedro::ui::core::NEW_VOICE;
            let vc = if nuova { pal.muted } else { cedro::ui::theme::voice(tema, *v as usize) };
            super::piastrella(p, pal, tema.dark(), 20.0, ry + 9.0, 36.0, if nuova { ic::ADD } else { "M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM4 20c0-4 4-6 8-6s8 2 8 6" }, vc, !nuova);
            p.text(nome, 70.0, ry, w - 90.0, rh, 16.0, Weight::Regular, pal.text, Align::Left);
            let az = match f {
                FoglioRiv::Voce(_) => RivAz::Voce(*v),
                FoglioRiv::Unisci(_) => RivAz::UnisciIn(*v),
            };
            self.colpi.push((Forma::R((0.0, ry, w, rh)), Azione::Riv(az)));
        }
    }

    /// Lo scorrimento delle frasi col dito.
    pub(super) fn scorri_rivedi(&mut self, dy: f32, da: f32) -> bool {
        match self.rivedi.as_mut() {
            Some(rv) => {
                rv.scorri = (da - dy).clamp(0.0, rv.scorri_max);
                true
            }
            None => false,
        }
    }

    pub(super) fn scorri_rivedi_ora(&self) -> f32 {
        self.rivedi.as_ref().map_or(0.0, |rv| rv.scorri)
    }
}


/// La scheda «Chi parla»: una riga per voce con il nome da scrivere, quanto parla, ▶ e «Unisci a…»; sotto,
/// «Scrivi chi parla nel file».
#[allow(clippy::too_many_arguments)]
fn chi_parla(p: &mut Painter, pal: &cedro::ui::theme::Palette, tema: cedro::ui::theme::ThemeId, rv: &mut Rivedi, r: super::R, campo: Option<super::pdf::Campo>, premuto: Option<Azione>, colpi: &mut Vec<(Forma, Azione)>) {
    let (x, mut y, cw) = (r.0, r.1, r.2);
    let scuro = tema.dark();
    let voci = rv.r.live_voices();
    for v in voci.iter().copied() {
        let vc = cedro::ui::theme::voice(tema, v as usize);
        super::sollevato(p, pal, scuro, x, y, cw, 112.0, 16.0);
        super::piastrella(p, pal, scuro, x + 14.0, y + 14.0, 40.0, "M12 12a4 4 0 1 0 0-8 4 4 0 0 0 0 8zM4 20c0-4 4-6 8-6s8 2 8 6", vc, true);
        // Il nome si scrive qui (la tastiera del telefono).
        let c = super::pdf::Campo::Voce(v);
        let attivo = campo == Some(c);
        let fr = (x + 64.0, y + 10.0, cw - 64.0 - 62.0, 46.0);
        p.rrect(fr.0, fr.1, fr.2, fr.3, 12.0, Some(pal.surface), Some((if attivo { vc } else { pal.border }, if attivo { 2.0 } else { 1.0 })));
        let nome = rv.r.names.get(v as usize).cloned().unwrap_or_default();
        let vis = if nome.is_empty() { trascrizione::speaker_name(v as usize) } else { nome.clone() };
        p.text(&vis, fr.0 + 12.0, fr.1, fr.2 - 24.0, fr.3, 16.0, Weight::Semibold, if nome.is_empty() { pal.muted } else { pal.text }, Align::Left);
        if attivo {
            let cx = fr.0 + 12.0 + if nome.is_empty() { 0.0 } else { p.measure(&nome, 16.0, Weight::Semibold) } + 1.0;
            p.fill(cx.min(fr.0 + fr.2 - 10.0), fr.1 + 11.0, 2.0, fr.3 - 22.0, vc);
        }
        colpi.push((Forma::R(fr), Azione::Campo(c)));
        // ▶: un pezzo della sua voce.
        let a = Azione::Riv(RivAz::Ascolta(v));
        let pc = (x + cw - 34.0, y + 33.0);
        p.circle(pc.0, pc.1, 22.0, Some(if premuto == Some(a) { mix(vc, (0, 0, 0), 0.15) } else { vc }), None);
        p.icon("M9 6l9 6-9 6z", pc.0 - 10.0, pc.1 - 12.0, 24.0, (255, 255, 255));
        colpi.push((Forma::Cerchio { cx: pc.0, cy: pc.1, r: 24.0 }, a));
        let (n, secs) = rv.r.voice_stats(v);
        let frasi = if n == 1 { "1 frase".to_string() } else { format!("{n} frasi") };
        let quanto = if secs >= 60.0 { format!("{frasi} · {} min", (secs / 60.0).round() as u64) } else { format!("{frasi} · {} s", secs.round() as u64) };
        p.text(&quanto, x + 64.0, y + 64.0, cw - 200.0, 36.0, 13.0, Weight::Regular, pal.muted, Align::Left);
        if voci.len() > 1 {
            let t = "Unisci a…";
            let bw = p.measure(t, 14.0, Weight::Semibold) + 28.0;
            let b = (x + cw - 14.0 - bw, y + 66.0, bw, 36.0);
            let a = Azione::Riv(RivAz::Unisci(v));
            p.rrect(b.0, b.1, b.2, b.3, 18.0, Some(if premuto == Some(a) { pal.alt } else { pal.surface }), Some((pal.border, 1.5)));
            p.text(t, b.0, b.1, b.2, b.3, 14.0, Weight::Semibold, pal.text, Align::Center);
            colpi.push((Forma::R((b.0, b.1 - 6.0, b.2, b.3 + 12.0)), a));
        }
        y += 122.0;
    }
    // Scrivi chi parla nel file.
    let t = "Scrivi chi parla nel file";
    p.text(t, x + 4.0, y + 4.0, cw - 70.0, 40.0, 15.0, Weight::Regular, pal.text, Align::Left);
    let on = rv.r.write_voices;
    let sx = x + cw - 54.0;
    p.rrect(sx, y + 10.0, 48.0, 28.0, 14.0, Some(if on { pal.primary } else { pal.border }), None);
    p.circle(if on { sx + 34.0 } else { sx + 14.0 }, y + 24.0, 10.0, Some((255, 255, 255)), None);
    colpi.push((Forma::R((x, y, cw, 48.0)), Azione::Riv(RivAz::ScriviVoci)));
    y += 52.0;
    p.text_wrap("Tocca un nome per scriverlo: cambia in tutte le frasi, nel file e nel riassunto. Il nome colorato su una frase sceglie chi la dice.", x + 4.0, y, cw - 8.0, 13.0, Weight::Regular, pal.muted, 19.0, 4);
}

/// La scheda «Riassunto»: il riassunto fatto (in breve, chi ha detto cosa, decisioni, cose da fare), oppure
/// il pulsante per farlo sul telefono, con quanto pesa e quanto ci mette.
#[allow(clippy::too_many_arguments)]
fn riassunto(p: &mut Painter, pal: &cedro::ui::theme::Palette, tema: cedro::ui::theme::ThemeId, rv: &mut Rivedi, r: super::R, premuto: Option<Azione>, colpi: &mut Vec<(Forma, Azione)>) {
    let (x, y0, cw, alto) = r;
    let h = y0 + alto;
    let scuro = tema.dark();
    let ai = mix((0x7A, 0x3F, 0xC8), (255, 255, 255), if scuro { 0.28 } else { 0.0 });
    let mut y = y0;
    // In corso: barra e passo.
    if let Some((_, prog)) = &rv.riassunto {
        // Oltre 1000: non si sa ancora (scarica il modello, lo carica): niente percentuale, barra vuota.
        let grezzo = prog.load(std::sync::atomic::Ordering::Relaxed);
        let noto = grezzo <= 1000;
        let m = if noto { grezzo } else { 0 };
        super::sollevato(p, pal, scuro, x, y, cw, 130.0, 16.0);
        super::piastrella(p, pal, scuro, x + 14.0, y + 14.0, 40.0, SCINTILLA, ai, true);
        p.text("Riassunto AI in corso", x + 64.0, y + 12.0, cw - 140.0, 26.0, 16.0, Weight::Semibold, pal.text, Align::Left);
        if noto {
            let pct = format!("{}%", m / 10);
            p.text(&pct, x + cw - 80.0, y + 10.0, 66.0, 34.0, 24.0, Weight::Semibold, ai, Align::Right);
        }
        p.text(&rv.riassunto_nota, x + 64.0, y + 38.0, cw - 78.0, 20.0, 13.0, Weight::Regular, pal.muted, Align::Left);
        let (bx, by, bw) = (x + 14.0, y + 74.0, cw - 28.0);
        p.rrect(bx, by, bw, 10.0, 5.0, Some(mix(ai, pal.surface, 0.8)), None);
        if noto {
            p.rrect(bx, by, (bw * m as f32 / 1000.0).max(10.0), 10.0, 5.0, Some(ai), None);
        }
        p.text_wrap("Il telefono può restare in tasca: a fine lavoro arriva una notifica.", bx, by + 18.0, bw, 12.0, Weight::Regular, pal.muted, 17.0, 2);
        return;
    }
    let Some(sez) = rv.r.summary_sections() else {
        let ch = 300.0;
        super::sollevato(p, pal, scuro, x, y, cw, ch, 16.0);
        super::piastrella(p, pal, scuro, x + 14.0, y + 14.0, 40.0, SCINTILLA, ai, true);
        p.text("Riassunto AI", x + 64.0, y + 12.0, cw - 78.0, 26.0, 17.0, Weight::Semibold, pal.text, Align::Left);
        p.text("Sul telefono, senza internet", x + 64.0, y + 36.0, cw - 78.0, 20.0, 13.0, Weight::Regular, pal.muted, Align::Left);
        let testo = "In breve, chi ha detto cosa, le decisioni e le cose da fare, scritti da Qwen3.5-4B come sul computer. \
                     La prima volta scarica il modello (2,7 GB, meglio col Wi-Fi); poi lavora anche senza rete.";
        let usata = p.text_wrap(testo, x + 14.0, y + 70.0, cw - 28.0, 14.0, Weight::Regular, pal.text, 20.0, 7);
        if !rv.riassunto_nota.is_empty() {
            p.text_wrap(&rv.riassunto_nota, x + 14.0, y + 76.0 + usata, cw - 28.0, 13.0, Weight::Regular, pal.bad, 18.0, 2);
        }
        let b = (x + 14.0, y + ch - 14.0 - 52.0, cw - 28.0, 52.0);
        let a = Azione::Riv(RivAz::Riassumi);
        super::lucido(p, scuro, b.0, b.1, b.2, b.3, 26.0, ai, premuto == Some(a));
        p.icon(SCINTILLA, b.0 + (b.2 - 190.0) / 2.0, b.1 + 14.0, 24.0, (255, 255, 255));
        p.text("Fai il riassunto", b.0 + (b.2 - 190.0) / 2.0 + 32.0, b.1, 160.0, b.3, 17.0, Weight::Semibold, (255, 255, 255), Align::Left);
        colpi.push((Forma::R(b), a));
        return;
    };
    // Il riassunto fatto, scorrendo con le schede delle quattro parti.
    let w_t = cw - 28.0;
    let parti: Vec<(&str, Vec<String>)> = vec![
        ("In breve", vec![sez.brief.clone()]),
        ("Chi ha detto cosa", sez.who.iter().map(|s| format!("{}: {}", s.name, s.text)).collect()),
        ("Decisioni", if sez.decisions.is_empty() { vec!["Nessuna decisione presa.".into()] } else { sez.decisions.clone() }),
    ];
    let piede = 120.0;
    let vista = (0.0, y0, x * 2.0 + cw, h - piede - y0);
    p.push_clip(vista.0, vista.1, vista.2, vista.3);
    y -= rv.scorri;
    for (titolo, righe) in &parti {
        p.text(titolo, x + 4.0, y, cw, 28.0, 15.0, Weight::Bold, ai, Align::Left);
        y += 30.0;
        for t in righe {
            y += p.text_wrap(t, x + 14.0, y, w_t, 15.0, Weight::Regular, pal.text, 22.0, 0) + 8.0;
        }
        y += 6.0;
    }
    p.text("Cose da fare", x + 4.0, y, cw, 28.0, 15.0, Weight::Bold, ai, Align::Left);
    y += 30.0;
    if sez.todo.is_empty() {
        y += p.text_wrap("Nessuna.", x + 14.0, y, w_t, 15.0, Weight::Regular, pal.text, 22.0, 0) + 8.0;
    }
    let fatte = rv.r.summary.as_ref().map(|s| s.done.clone()).unwrap_or_default();
    for (k, t) in sez.todo.iter().enumerate() {
        let on = fatte.contains(&k);
        if on {
            p.rrect(x + 14.0, y + 1.0, 22.0, 22.0, 5.0, Some(ai), None);
            p.icon("M5 12l5 5 9-10", x + 17.0, y + 4.0, 16.0, (255, 255, 255));
        } else {
            p.rrect(x + 14.0, y + 1.0, 22.0, 22.0, 5.0, Some(pal.surface), Some((pal.muted, 1.5)));
        }
        let testo = match &t.owner { Some(o) if !o.trim().is_empty() => format!("{} ({o})", t.what), _ => t.what.clone() };
        let alto = p.text_wrap(&testo, x + 46.0, y, w_t - 32.0, 15.0, Weight::Regular, if on { pal.muted } else { pal.text }, 22.0, 0);
        if let Some(r) = super::intersezione((x, y - 6.0, cw, alto + 12.0), vista) {
            colpi.push((Forma::R(r), Azione::Riv(RivAz::FattoTodo(k))));
        }
        y += alto + 10.0;
    }
    if let Some(s) = &rv.r.summary {
        let secs = s.ms / 1000;
        let dove = s.gpu.clone().unwrap_or_else(|| "processore".into());
        y += 6.0;
        y += p.text_wrap(&format!("{} · fatto in {} min {:02} s sul {dove}. {}", cedro::riassunto::MODEL_NAME, secs / 60, secs % 60, cedro::riassunto::WARNING), x + 4.0, y, cw - 8.0, 12.0, Weight::Regular, pal.muted, 18.0, 3);
    }
    p.pop_clip();
    rv.scorri_max = (y + rv.scorri - (vista.1 + vista.3) + 8.0).max(0.0);
    rv.scorri = rv.scorri.min(rv.scorri_max);
    // Il piede: in cima al file, e rifai.
    let fy = h - piede + 8.0;
    let on = rv.r.summary_on_top;
    p.text("Metti il riassunto in cima al file", x + 4.0, fy, cw - 70.0, 40.0, 15.0, Weight::Regular, pal.text, Align::Left);
    let sx = x + cw - 54.0;
    p.rrect(sx, fy + 6.0, 48.0, 28.0, 14.0, Some(if on { pal.primary } else { pal.border }), None);
    p.circle(if on { sx + 34.0 } else { sx + 14.0 }, fy + 20.0, 10.0, Some((255, 255, 255)), None);
    colpi.push((Forma::R((x, fy, cw, 44.0)), Azione::Riv(RivAz::RiassuntoInCima)));
    let b = (x, fy + 52.0, cw, 48.0);
    let a = Azione::Riv(RivAz::Riassumi);
    p.rrect(b.0, b.1, b.2, b.3, 24.0, Some(if premuto == Some(a) { pal.alt } else { pal.surface }), Some((ai, 2.0)));
    p.text("Rifai il riassunto", b.0, b.1, b.2, b.3, 15.0, Weight::Semibold, ai, Align::Center);
    colpi.push((Forma::R(b), a));
}

/// La scintilla del Riassunto AI.
const SCINTILLA: &str = "M12 3l1.8 5.2L19 10l-5.2 1.8L12 17l-1.8-5.2L5 10l5.2-1.8zM19 15l.8 2.2L22 18l-2.2.8L19 21l-.8-2.2L16 18l2.2-.8z";
