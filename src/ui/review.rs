//! «Rivedi» la trascrizione: le frasi si ascoltano dal punto che si clicca, si correggono sul posto e
//! i difetti tipici di whisper (ripetizioni, frasi inventate nel silenzio) si segnalano e si sistemano.
//! Qui c'è lo stato e quel che fa; il disegno sta in `views/revisione.rs`.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use super::model::{App, Event, Notice};
use crate::revisione::{self, Flag, Kind, Wave};
use crate::trascrizione::{self, Segment};

/// Una frase della trascrizione, con quel che ne è stato fatto.
#[derive(Clone)]
pub struct Row {
    pub t0: i64,
    pub t1: i64,
    pub text: String,
    /// Il testo di whisper, per sapere cosa è stato corretto.
    pub orig: String,
    pub removed: bool,
}

/// In cosa si salva la revisione.
pub const FORMATS: [(&str, &str); 4] = [("Word", "docx"), ("Testo", "txt"), (".srt", "srt"), (".vtt", "vtt")];

pub struct Review {
    pub source: PathBuf,
    pub secs: f64,
    pub lang: String,
    pub rows: Vec<Row>,
    pub flags: Vec<Flag>,
    /// L'onda dell'audio: arriva dopo, quando è stata letta tutta.
    pub bars: Option<Vec<f32>>,
    /// Il punto da controllare scelto nell'elenco.
    pub cur: usize,
    /// Quelli che si è deciso di lasciare com'erano.
    pub dismissed: HashSet<usize>,
    pub only_flagged: bool,
    pub follow: bool,
    pub format: usize,
    undo: Vec<Vec<(String, bool)>>,
    /// Il file scritto l'ultima volta, e cosa è successo.
    pub saved: Option<PathBuf>,
    pub note: String,
    /// La frase che si sta seguendo (da `follow`): quando cambia, la lista scorre.
    pub followed: Option<usize>,
    /// La frase aperta alla correzione.
    pub sel: Option<usize>,
}

impl Review {
    pub fn new(source: &Path, secs: f64, lang: String, segments: Vec<Segment>) -> Review {
        let rows: Vec<Row> = segments.into_iter().map(|s| Row { t0: s.t0, t1: s.t1, orig: s.text.clone(), text: s.text, removed: false }).collect();
        let mut r = Review {
            source: source.to_path_buf(),
            secs,
            lang,
            rows,
            flags: Vec::new(),
            bars: None,
            cur: 0,
            dismissed: HashSet::new(),
            only_flagged: false,
            follow: true,
            format: 0,
            undo: Vec::new(),
            saved: None,
            note: String::new(),
            followed: None,
            sel: None,
        };
        r.detect();
        r
    }

    /// Cerca i punti da controllare sul testo di whisper (le correzioni non li cambiano).
    pub fn detect(&mut self) {
        let orig: Vec<Segment> = self.rows.iter().map(|r| Segment { t0: r.t0, t1: r.t1, text: r.orig.clone() }).collect();
        let wave = self.bars.as_deref().map(|bars| Wave { bars, secs: self.secs });
        self.flags = revisione::find(&orig, wave.as_ref());
        self.dismissed.clear();
        self.cur = 0;
    }

    /// Le frasi che restano, con le correzioni.
    pub fn segments(&self) -> Vec<Segment> {
        self.rows.iter().filter(|r| !r.removed).map(|r| Segment { t0: r.t0, t1: r.t1, text: r.text.trim().to_string() }).filter(|s| !s.text.is_empty()).collect()
    }

    /// Il punto è ancora da controllare: nessuno l'ha sistemato o lasciato com'era.
    pub fn is_open(&self, i: usize) -> bool {
        let Some(f) = self.flags.get(i) else { return false };
        if self.dismissed.contains(&i) {
            return false;
        }
        match f.kind {
            Kind::Repeat => f.segs.iter().skip(1).any(|s| self.rows.get(*s).is_some_and(|r| !r.removed)),
            Kind::Loop => f.segs.first().and_then(|s| self.rows.get(*s)).is_some_and(|r| !r.removed && revisione::collapse_loops(&r.text).is_some()),
            Kind::Typical | Kind::Silence => f.segs.first().and_then(|s| self.rows.get(*s)).is_some_and(|r| !r.removed && r.text == r.orig),
        }
    }

    pub fn open_count(&self) -> usize {
        (0..self.flags.len()).filter(|i| self.is_open(*i)).count()
    }

    /// Il punto da controllare che comprende la frase `row`, se c'è.
    pub fn flag_of(&self, row: usize) -> Option<usize> {
        self.flags.iter().position(|f| f.segs.contains(&row))
    }

    pub fn push_undo(&mut self) {
        self.undo.push(self.rows.iter().map(|r| (r.text.clone(), r.removed)).collect());
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn undo(&mut self) {
        if let Some(state) = self.undo.pop() {
            for (r, (text, removed)) in self.rows.iter_mut().zip(state) {
                r.text = text;
                r.removed = removed;
            }
        }
    }

    /// Sistema un punto: tiene una sola ripetizione, riduce il giro di parole, toglie la frase inventata.
    pub fn fix(&mut self, i: usize) {
        let Some(flag) = self.flags.get(i).cloned() else { return };
        match flag.kind {
            Kind::Repeat => {
                for s in flag.segs.iter().skip(1) {
                    if let Some(r) = self.rows.get_mut(*s) {
                        r.removed = true;
                    }
                }
            }
            Kind::Loop => {
                if let Some(r) = flag.segs.first().and_then(|s| self.rows.get_mut(*s)) {
                    if let Some((text, _)) = revisione::collapse_loops(&r.text) {
                        r.text = text;
                    }
                }
            }
            Kind::Typical | Kind::Silence => {
                if let Some(r) = flag.segs.first().and_then(|s| self.rows.get_mut(*s)) {
                    r.removed = true;
                }
            }
        }
    }

    /// Sistema tutte le ripetizioni (frasi e giri di parole); le frasi nel silenzio le decide chi rivede.
    pub fn fix_repeats(&mut self) {
        for i in 0..self.flags.len() {
            if matches!(self.flags[i].kind, Kind::Repeat | Kind::Loop) && self.is_open(i) {
                self.fix(i);
            }
        }
    }

    /// Il punto aperto prima o dopo quello scelto (`step` -1 o 1), girando intorno.
    pub fn step_flag(&mut self, step: i32) {
        let n = self.flags.len();
        if n == 0 {
            return;
        }
        let from = self.cur.min(n - 1);
        let order: Vec<usize> = (1..=n).map(|k| if step >= 0 { (from + k) % n } else { (from + n - k % n) % n }).collect();
        self.cur = order.iter().copied().find(|i| self.is_open(*i)).unwrap_or_else(|| order[0]);
    }

    /// La frase che sta suonando a `secs`.
    pub fn row_at(&self, secs: f64) -> Option<usize> {
        let t = (secs * 100.0) as i64;
        let next = self.rows.iter().position(|r| r.t0 > t).unwrap_or(self.rows.len());
        next.checked_sub(1)
    }

    /// Le frasi da mostrare: tutte, o solo quelle dei punti da controllare.
    pub fn visible(&self) -> Vec<usize> {
        (0..self.rows.len()).filter(|i| !self.only_flagged || self.flag_of(*i).is_some()).collect()
    }
}

/// Altezza di una frase nell'elenco.
pub const ROW_H: f32 = 70.0;

impl App {
    /// Apre la revisione della trascrizione del file `index` della coda.
    pub fn review_open(&mut self, index: usize) {
        let Some(path) = self.files.get(index).map(|f| f.path.clone()) else { return };
        let Some(saved) = revisione::load(&path) else {
            self.notice = Some(Notice { text: "La revisione di questo file non c'è più: trascrivilo di nuovo.".into(), error: true });
            return;
        };
        let secs = saved.secs;
        self.notice = None;
        self.review = Some(Review::new(&path, secs, saved.lang, saved.segments));
        self.review_scrub = None;
        // L'onda si legge in un thread: sull'audio lungo ci vuole un po', e serve anche a trovare le frasi nel silenzio.
        let (tx, wake) = (self.tx.clone(), self.wake.clone());
        std::thread::spawn(move || {
            let n = (secs as usize).clamp(300, 6000);
            let bars = crate::audio::waveform(&path, n);
            let _ = tx.send(Event::Wave(path, bars));
            wake();
        });
    }

    /// Chiude la revisione e ferma l'audio.
    pub fn review_close(&mut self) {
        if let Some(r) = self.review.take() {
            if self.player.as_ref().is_some_and(|p| p.path == r.source) {
                self.player = None;
            }
        }
        self.review_scrub = None;
    }

    pub(super) fn review_wave(&mut self, path: &Path, bars: Option<Vec<f32>>) {
        if let Some(r) = self.review.as_mut().filter(|r| r.source == path) {
            r.bars = bars;
            if r.bars.is_some() {
                // Con l'onda si trovano anche le frasi nel silenzio; le scelte già fatte restano valide perché
                // i punti già trovati non cambiano ordine (si aggiungono soltanto).
                let before: Vec<(Kind, Vec<usize>)> = r.flags.iter().map(|f| (f.kind, f.segs.clone())).collect();
                let dismissed: Vec<(Kind, Vec<usize>)> = r.dismissed.iter().filter_map(|i| before.get(*i).cloned()).collect();
                r.detect();
                r.dismissed = r.flags.iter().enumerate().filter(|(_, f)| dismissed.iter().any(|(k, s)| *k == f.kind && *s == f.segs)).map(|(i, _)| i).collect();
            }
        }
    }

    fn review_player(&mut self) -> Option<&mut super::player::Player> {
        let (source, secs) = self.review.as_ref().map(|r| (r.source.clone(), r.secs))?;
        if self.player.as_ref().is_none_or(|p| p.path != source) {
            self.player = Some(super::player::Player::start(&source, false, Some(secs).filter(|s| *s > 0.0), self.wake.clone()));
        }
        self.player.as_mut()
    }

    fn review_audio(&self) -> Option<&super::player::Player> {
        let r = self.review.as_ref()?;
        self.player.as_ref().filter(|p| p.path == r.source)
    }

    pub fn review_playing(&self) -> bool {
        self.review_audio().is_some_and(|p| p.playing())
    }

    pub fn review_loading(&self) -> bool {
        self.review_audio().is_some_and(|p| p.loading())
    }

    pub fn review_position(&self) -> f64 {
        self.review_audio().map_or(0.0, |p| p.position())
    }

    /// Play o pausa; la prima volta parte dalla frase scelta.
    pub fn review_toggle(&mut self) {
        let start = self.review.as_ref().and_then(|r| r.sel.and_then(|i| r.rows.get(i)).map(|row| row.t0 as f64 / 100.0));
        if self.review_audio().is_none() {
            if let Some(p) = self.review_player() {
                // Appena acceso parte dall'inizio: si porta al punto scelto.
                if let Some(s) = start {
                    p.seek(s);
                }
            }
            return;
        }
        if let Some(p) = self.player.as_mut() {
            p.toggle();
        }
    }

    /// Fa sentire dal secondo `secs`.
    pub fn review_play_at(&mut self, secs: f64) {
        let fresh = self.review_audio().is_none();
        if let Some(p) = self.review_player() {
            p.seek(secs.max(0.0));
            // Se era in pausa riparte.
            if !fresh && !p.playing() {
                p.toggle();
            }
        }
    }

    pub fn review_skip(&mut self, delta: i32) {
        let to = self.review_position() + delta as f64;
        let max = self.review.as_ref().map_or(0.0, |r| r.secs);
        self.review_play_at(to.clamp(0.0, (max - 0.1).max(0.0)));
    }

    pub fn review_seek(&mut self, frac: f32) {
        let secs = self.review.as_ref().map_or(0.0, |r| r.secs);
        self.review_play_at(secs * frac.clamp(0.0, 1.0) as f64);
    }

    /// Il tempo di una frase: la sceglie e la fa sentire da lì.
    pub fn review_row_play(&mut self, row: usize) {
        let Some(r) = self.review.as_mut() else { return };
        let Some(t0) = r.rows.get(row).map(|x| x.t0) else { return };
        r.sel = Some(row);
        r.followed = Some(row);
        self.review_play_at(t0 as f64 / 100.0);
    }

    /// Sceglie un punto da controllare: va alla sua prima frase e la fa sentire.
    pub fn review_go_flag(&mut self, i: usize) {
        let Some(r) = self.review.as_mut() else { return };
        let Some(start) = r.flags.get(i).and_then(|f| f.segs.first().copied()) else { return };
        r.cur = i;
        self.review_row_play(start);
    }

    pub fn review_step(&mut self, step: i32) {
        let Some(r) = self.review.as_mut() else { return };
        r.step_flag(step);
        let i = r.cur;
        self.review_go_flag(i);
    }

    /// Scrive la revisione nel formato scelto, accanto all'originale (o nella cartella scelta).
    pub fn review_save(&mut self) {
        let dest = self.dest.clone();
        let Some(r) = self.review.as_mut() else { return };
        let segments = r.segments();
        let ext = FORMATS[r.format.min(FORMATS.len() - 1)].1;
        let bytes = match ext {
            "docx" => trascrizione::docx_bytes(&r.source, r.secs, &r.lang, &segments),
            "txt" => trascrizione::text_bytes(&r.source, r.secs, &r.lang, &segments),
            "srt" => revisione::srt_bytes(&segments),
            _ => revisione::vtt_bytes(&segments),
        };
        let dir = dest.or_else(|| r.source.parent().filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf)).unwrap_or_else(|| PathBuf::from("."));
        let stem = r.source.file_stem().and_then(|s| s.to_str()).unwrap_or("trascrizione");
        let mut out = dir.join(format!("{stem} - rivisto.{ext}"));
        let mut n = 2;
        while out.exists() {
            out = dir.join(format!("{stem} - rivisto ({n}).{ext}"));
            n += 1;
        }
        match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&out, bytes)) {
            Ok(()) => {
                // Le frasi corrette restano: la prossima volta si riparte da qui.
                let _ = revisione::save(&r.source, r.secs, &r.lang, &segments);
                r.note = format!("Salvato: {}", out.file_name().and_then(|n| n.to_str()).unwrap_or(""));
                r.saved = Some(out);
            }
            Err(e) => {
                self.notice = Some(Notice { text: format!("{} non scritto: {e}", out.display()), error: true });
            }
        }
    }

    /// Ogni fotogramma: con «Segui» attivo, l'elenco scorre fino alla frase che suona. Dà la nuova posizione dell'elenco.
    pub fn review_follow(&mut self) -> Option<f32> {
        let playing = self.review_playing();
        let pos = self.review_position();
        let r = self.review.as_mut()?;
        if !(playing && r.follow) {
            return None;
        }
        let row = r.row_at(pos)?;
        if r.followed == Some(row) {
            return None;
        }
        r.followed = Some(row);
        let at = r.visible().iter().position(|i| *i == row)?;
        Some((at as f32 * ROW_H - ROW_H * 2.0).max(0.0))
    }

    /// Cosa fare dello scorrimento dopo un salto a mano alla frase `row`.
    pub fn review_scroll_to(&self, row: usize) -> Option<f32> {
        let r = self.review.as_ref()?;
        let at = r.visible().iter().position(|i| *i == row)?;
        Some((at as f32 * ROW_H - ROW_H * 2.0).max(0.0))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(t0: i64, text: &str) -> Segment {
        Segment { t0, t1: t0 + 500, text: text.to_string() }
    }

    fn sample() -> Review {
        let mut segments = vec![seg(0, "Buongiorno a tutti."), seg(600, "Il bilancio è stato approvato."), seg(1200, "Il bilancio è stato approvato."), seg(1800, "Il bilancio è stato approvato."), seg(2400, "Sì sì sì sì sì sì sì va bene.")];
        segments.push(seg(3000, "Grazie e arrivederci."));
        Review::new(Path::new("/tmp/prova.wav"), 40.0, "it".into(), segments)
    }

    #[test]
    fn fixing_repeats_keeps_one_and_undo_restores() {
        let mut r = sample();
        assert!(r.open_count() >= 2);
        r.push_undo();
        r.fix_repeats();
        assert_eq!(r.open_count(), 0);
        let kept: Vec<String> = r.segments().into_iter().map(|s| s.text).collect();
        assert_eq!(kept, ["Buongiorno a tutti.", "Il bilancio è stato approvato.", "Sì va bene.", "Grazie e arrivederci."]);
        r.undo();
        assert_eq!(r.segments().len(), 6);
        assert!(r.open_count() >= 2);
    }

    #[test]
    fn row_at_follows_the_clock() {
        let r = sample();
        assert_eq!(r.row_at(0.0), Some(0));
        assert_eq!(r.row_at(7.0), Some(1));
        assert_eq!(r.row_at(100.0), Some(5));
    }

    #[test]
    fn only_flagged_shows_the_flagged_rows() {
        let mut r = sample();
        r.only_flagged = true;
        let v = r.visible();
        assert!(!v.contains(&0));
        assert!(v.contains(&1) && v.contains(&4));
    }
}
