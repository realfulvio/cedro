//! «Rivedi trascrizione»: le frasi con il tempo, l'ascolto dal punto che si tocca, i difetti tipici di whisper
//! segnalati (ripetizioni, giri di parole, frasi inventate nel silenzio) e il salvataggio in TXT, SRT o VTT.
//! Lo stato e le correzioni sono quelli del computer (`cedro::ui::review::Review`); qui c'è il disegno per il tocco.

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
        self.rivedi = Some(Rivedi { r: Review::new(&src, secs, s.lang, s.segments, s.names, s.summary), titolo, player: None, salva: None, nota: String::new(), altezze: Vec::new(), largo: 0.0, scorri: 0.0, scorri_max: 0.0, onda: (0.0, 0.0) });
        Effetto::OndaRevisione(src, secs)
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

    /// Un salvataggio della revisione è finito.
    pub(super) fn salvata(&mut self, id: u32, stato: &Stato) {
        if let Some(rv) = self.rivedi.as_mut().filter(|rv| rv.salva == Some(id)) {
            rv.salva = None;
            rv.nota = match stato {
                Stato::Fatto { nome, .. } => format!("Salvato in {nome}"),
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
                let (ext, dati) = match k {
                    1 => ("srt", revisione::srt_bytes(&segs, &rv.r.names)),
                    2 => ("vtt", revisione::vtt_bytes(&segs, &rv.r.names)),
                    _ => ("txt", trascrizione::text_bytes(&rv.r.source, rv.r.secs, &rv.r.lang, &segs, &rv.r.names)),
                };
                // Le correzioni restano: la prossima volta si riparte da qui.
                let _ = revisione::save(&rv.r.source, rv.r.secs, &rv.r.lang, &segs, &rv.r.names);
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
        p.rrect(x, y, cw, card_h, 20.0, Some(pal.surface), Some((pal.border, 1.0)));
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
                    16.0 + righe * 22.0 + if aperto { 50.0 } else { 0.0 }
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
            let usata = p.text_wrap(&row.text, x + 70.0, ry + 8.0, tw, 15.0, Weight::Regular, col, 22.0, 0);
            if row.removed {
                for l in 0..(usata / 22.0).round().max(1.0) as usize {
                    p.fill(x + 70.0, ry + 8.0 + l as f32 * (usata / (usata / 22.0).round().max(1.0)) + 12.0, tw.min(p.measure(&row.text, 15.0, Weight::Regular)), 1.5, pal.muted);
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
