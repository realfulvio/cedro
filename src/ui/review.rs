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
    /// Chi la dice (indice in `Review::names`), se le voci sono state riconosciute.
    pub speaker: Option<u8>,
}

/// Lo stato da rimettere con «Ripristina»: le frasi (testo, tolta, chi parla) e i nomi.
type Snapshot = (Vec<(String, bool, Option<u8>)>, Vec<String>);

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
    /// L'onda non si è potuta leggere (audio spostato o illeggibile): niente rotellina che gira per sempre.
    pub wave_failed: bool,
    /// Il punto da controllare scelto nell'elenco.
    pub cur: usize,
    /// Quelli che si è deciso di lasciare com'erano.
    pub dismissed: HashSet<usize>,
    pub only_flagged: bool,
    pub follow: bool,
    pub format: usize,
    undo: Vec<Snapshot>,
    /// I nomi delle voci (vuoto: la trascrizione è senza voci).
    pub names: Vec<String>,
    /// Scrivere chi parla nel file salvato.
    pub write_voices: bool,
    /// La scheda a destra: 0 «Da controllare», 1 «Chi parla», 2 «Riassunto».
    pub tab: u8,
    /// Il Riassunto AI, se è stato fatto.
    pub summary: Option<crate::riassunto::Stored>,
    /// Mettere il riassunto in cima al file salvato (Word e testo).
    pub summary_on_top: bool,
    /// «Copia» appena premuto: il pulsante dice «Copiato».
    pub summary_copied: bool,
    /// Il file scritto l'ultima volta, e cosa è successo.
    pub saved: Option<PathBuf>,
    pub note: String,
    /// La frase che si sta seguendo (da `follow`): quando cambia, la lista scorre.
    pub followed: Option<usize>,
    /// La frase aperta alla correzione.
    pub sel: Option<usize>,
    /// È un referto (dettatura veterinaria): le frasi con numeri e parole decisive vanno riascoltate e
    /// confermate, e finché ne resta una il file esce come bozza.
    pub referto: bool,
    pub view: u8,
    restored_checked: HashSet<String>,
}

impl Review {
    pub fn new(source: &Path, secs: f64, lang: String, segments: Vec<Segment>, names: Vec<String>, summary: Option<crate::riassunto::Stored>) -> Review {
        let rows: Vec<Row> = segments
            .into_iter()
            .map(|s| Row { t0: s.t0, t1: s.t1, orig: s.text.clone(), text: s.text, removed: false, speaker: s.speaker.filter(|v| (*v as usize) < names.len()) })
            .collect();
        let mut r = Review {
            source: source.to_path_buf(),
            secs,
            lang,
            rows,
            flags: Vec::new(),
            bars: None,
            wave_failed: false,
            cur: 0,
            dismissed: HashSet::new(),
            only_flagged: false,
            follow: true,
            format: 0,
            undo: Vec::new(),
            names,
            write_voices: true,
            tab: 0,
            summary_on_top: summary.is_some(),
            summary_copied: false,
            summary,
            saved: None,
            note: String::new(),
            followed: None,
            sel: None,
            referto: false,
            view: 1,
            restored_checked: HashSet::new(),
        };
        r.detect();
        // Con le voci si parte da «Chi parla» (i nomi da dare), a meno che ci sia qualcosa da controllare.
        if !r.names.is_empty() && r.open_count() == 0 {
            r.tab = 1;
        }
        r
    }

    pub fn from_saved(source: &Path, saved: revisione::Saved) -> Self {
        let mut r = Self::new(source, saved.secs, saved.lang, saved.segments, saved.names, saved.summary);
        r.referto = saved.review.referto;
        r.view = saved.review.view.min(2);
        r.restored_checked = saved.review.checked.into_iter().collect();
        r.detect();
        if r.open_count() > 0 { r.tab = 0; }
        r
    }

    fn flag_key(&self, i: usize) -> String {
        let f = &self.flags[i];
        let rows: Vec<_> = f.segs.iter().filter_map(|i| self.rows.get(*i)).map(|r| (r.t0, r.t1, r.text.as_str())).collect();
        format!("{:?}:{}:{}", f.kind, f.snippet, serde_json::to_string(&rows).unwrap_or_default())
    }

    pub fn persist(&self) -> std::io::Result<()> {
        let segments=self.segments();
        let mut next=Self::new(&self.source,self.secs,self.lang.clone(),segments.clone(),self.names.clone(),self.summary.clone());
        next.referto=self.referto;next.bars=self.bars.clone();next.detect();
        let checked:HashSet<_>=(0..self.flags.len()).filter(|i|!self.is_open(*i)).map(|i|self.flag_key(i)).chain(self.restored_checked.iter().cloned()).collect();
        let checked=(0..next.flags.len()).filter(|i| {
            checked.contains(&next.flag_key(*i)) || next.flags[*i].segs.iter().all(|j| {
                let row=&next.rows[*j];
                self.rows.iter().enumerate().any(|(old_i,old)|old.t0==row.t0&&old.t1==row.t1&&old.text.trim()==row.text&&old.text!=old.orig &&
                    !self.flags.iter().enumerate().any(|(k,f)| f.segs.contains(&old_i)&&f.snippet==next.flags[*i].snippet&&self.is_open(k)))
            })
        }).map(|i|next.flag_key(i)).collect();
        let state=revisione::ReviewState{referto:self.referto,view:self.view,checked};
        revisione::save_review(&self.source, self.secs, &self.lang, &self.segments(), &self.names, self.summary.as_ref(), &state)
    }

    /// Un solo esportatore per telefono e computer, compresi nome e avviso dei sottotitoli.
    pub fn export(&self, ext: &str) -> (String, Vec<u8>) {
        let segments = self.segments();
        let names: &[String] = if self.write_voices { &self.names } else { &[] };
        let summary = self.summary_sections().filter(|_| self.summary_on_top);
        let draft = self.draft_line();
        let bytes = match ext {
            "docx" => trascrizione::docx_bytes_full(&self.source, self.secs, &self.lang, &segments, names, summary.as_ref(), draft.as_deref()),
            "srt" => revisione::srt_bytes_full(&segments, names, draft.as_deref()),
            "vtt" => revisione::vtt_bytes_full(&segments, names, draft.as_deref()),
            _ => trascrizione::text_bytes_full(&self.source, self.secs, &self.lang, &segments, names, summary.as_ref(), draft.as_deref()),
        };
        let stem = self.source.file_stem().and_then(|s| s.to_str()).unwrap_or("trascrizione");
        let tag = if draft.is_some() { "bozza" } else { "rivisto" };
        (format!("{stem} - {tag}.{ext}"), bytes)
    }

    /// Cerca i punti da controllare sul testo di whisper (le correzioni non li cambiano).
    pub fn detect(&mut self) {
        let orig: Vec<Segment> = self.rows.iter().map(|r| Segment { t0: r.t0, t1: r.t1, text: r.orig.clone(), speaker: r.speaker }).collect();
        let wave = self.bars.as_deref().map(|bars| Wave { bars, secs: self.secs });
        self.flags = revisione::find(&orig, wave.as_ref());
        if self.referto {
            // Le frasi già segnate per i difetti di whisper non si segnano due volte.
            let taken: HashSet<usize> = self.flags.iter().flat_map(|f| f.segs.iter().copied()).collect();
            self.flags.extend(revisione::find_referto(&orig).into_iter().filter(|f| f.segs.iter().all(|s| !taken.contains(s))));
            self.flags.sort_by_key(|f| f.segs.first().copied().unwrap_or(0));
        }
        self.dismissed = (0..self.flags.len()).filter(|i|self.restored_checked.contains(&self.flag_key(*i))).collect();
        self.cur = 0;
    }

    /// Le frasi che restano, con le correzioni.
    pub fn segments(&self) -> Vec<Segment> {
        self.rows
            .iter()
            .filter(|r| !r.removed)
            .map(|r| Segment { t0: r.t0, t1: r.t1, text: r.text.trim().to_string(), speaker: r.speaker })
            .filter(|s| !s.text.is_empty())
            .collect()
    }

    /// Le voci che hanno ancora almeno una frase (dopo «Unisci» qualcuna resta vuota e non si mostra).
    pub fn live_voices(&self) -> Vec<u8> {
        (0..self.names.len() as u8).filter(|v| self.rows.iter().any(|r| !r.removed && r.speaker == Some(*v))).collect()
    }

    /// Il nome della voce `v` (quello di partenza se è stato cancellato).
    pub fn name(&self, v: u8) -> String {
        trascrizione::name_of(&self.names, Some(v)).unwrap_or_else(|| trascrizione::speaker_name(v as usize))
    }

    /// I nomi di adesso, uno per voce (quello di partenza dove è stato cancellato).
    pub fn current_names(&self) -> Vec<String> {
        (0..self.names.len() as u8).map(|v| self.name(v)).collect()
    }

    /// Il riassunto pronto da mostrare, con i nomi di adesso.
    pub fn summary_sections(&self) -> Option<crate::riassunto::Sections> {
        self.summary.as_ref().map(|s| s.sections(&self.current_names()))
    }

    /// Quante frasi e quanti secondi di parlato ha la voce `v`.
    pub fn voice_stats(&self, v: u8) -> (usize, f64) {
        self.rows.iter().filter(|r| !r.removed && r.speaker == Some(v)).fold((0, 0.0), |(n, s), r| (n + 1, s + (r.t1 - r.t0).max(0) as f64 / 100.0))
    }

    /// Un pezzo da far sentire della voce `v`: la sua frase più lunga (la prima, a parità).
    pub fn voice_sample(&self, v: u8) -> Option<usize> {
        (0..self.rows.len()).filter(|i| !self.rows[*i].removed && self.rows[*i].speaker == Some(v)).max_by_key(|i| (self.rows[*i].t1 - self.rows[*i].t0, std::cmp::Reverse(*i)))
    }

    /// Cambia chi dice la frase `row`. `NEW_VOICE` crea una voce nuova (o riusa una rimasta senza frasi).
    pub fn set_speaker(&mut self, row: usize, v: u8) {
        if row >= self.rows.len() {
            return;
        }
        let v = if v == crate::ui::core::NEW_VOICE {
            let live = self.live_voices();
            if self.names.len() < trascrizione::MAX_VOICES {
                self.names.push(trascrizione::speaker_name(self.names.len()));
                self.names.len() as u8 - 1
            } else if let Some(free) = (0..self.names.len() as u8).find(|v| !live.contains(v) && self.rows[row].speaker != Some(*v)) {
                self.names[free as usize] = trascrizione::speaker_name(free as usize);
                free
            } else {
                return;
            }
        } else if (v as usize) < self.names.len() {
            v
        } else {
            return;
        };
        self.rows[row].speaker = Some(v);
    }

    /// «Unisci a…»: tutte le frasi della voce `from` passano a `into`.
    pub fn merge(&mut self, from: u8, into: u8) {
        if from == into || into as usize >= self.names.len() {
            return;
        }
        for r in self.rows.iter_mut().filter(|r| r.speaker == Some(from)) {
            r.speaker = Some(into);
        }
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
            // Riascoltata e corretta a mano (la frase non c'è più com'era) vale come controllata; se no serve «Confermo».
            Kind::Number | Kind::Meaning => f.segs.first().and_then(|s| self.rows.get(*s)).is_some_and(|r| !r.removed && (if f.snippet.is_empty() { r.text == r.orig } else { r.text.contains(&f.snippet) })),
        }
    }

    /// I punti del referto ancora da riascoltare (numeri e parole decisive).
    pub fn draft_points(&self) -> Vec<usize> {
        (0..self.flags.len()).filter(|i| matches!(self.flags[*i].kind, Kind::Number | Kind::Meaning) && self.is_open(*i)).collect()
    }

    /// Il referto esce come bozza: c'è ancora qualcosa da riascoltare.
    pub fn is_draft(&self) -> bool {
        self.referto && self.open_count() > 0
    }

    /// La riga in cima alla bozza: quanti punti e dove.
    pub fn draft_line(&self) -> Option<String> {
        if !self.is_draft() {
            return None;
        }
        let open: Vec<&Flag> = (0..self.flags.len()).filter(|i| self.is_open(*i)).map(|i| &self.flags[i]).collect();
        let rows: Vec<Segment> = self.rows.iter().map(|r| Segment { t0: r.t0, t1: r.t1, text: String::new(), speaker: None }).collect();
        revisione::riga_bozza(&rows, &open)
    }

    pub fn open_count(&self) -> usize {
        (0..self.flags.len()).filter(|i| self.is_open(*i)).count()
    }

    /// Il punto da controllare che comprende la frase `row`, se c'è.
    pub fn flag_of(&self, row: usize) -> Option<usize> {
        // Una frase può averne più d'uno (un referto con più frasi in un pezzo): prima quelli aperti.
        let mut tutti = (0..self.flags.len()).filter(|i| self.flags[*i].segs.contains(&row));
        let primo = tutti.next()?;
        Some(std::iter::once(primo).chain(tutti).find(|i| self.is_open(*i)).unwrap_or(primo))
    }

    pub fn push_undo(&mut self) {
        self.undo.push((self.rows.iter().map(|r| (r.text.clone(), r.removed, r.speaker)).collect(), self.names.clone()));
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn undo(&mut self) {
        if let Some((rows, names)) = self.undo.pop() {
            for (r, (text, removed, speaker)) in self.rows.iter_mut().zip(rows) {
                r.text = text;
                r.removed = removed;
                r.speaker = speaker;
            }
            self.names = names;
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
            // Riascoltata e giusta: si conferma.
            Kind::Number | Kind::Meaning => {
                self.dismissed.insert(i);
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
        let mut rv = Review::from_saved(&path, saved);
        if self.transcribe.glossario_vet {
            rv.referto = true;
            rv.detect();
            if rv.open_count() > 0 {
                rv.tab = 0;
            }
        }
        self.review = Some(rv);
        self.review_scrub = None;
        // L'onda si legge in un thread: sull'audio lungo ci vuole un po', e serve anche a trovare le frasi nel silenzio.
        let (tx, wake) = (self.tx.clone(), self.wake.clone());
        std::thread::spawn(move || {
            let n = (secs as usize).clamp(300, 6000);
            let bars = std::panic::catch_unwind(|| crate::audio::waveform(&path, n)).ok().flatten();
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
            r.wave_failed = bars.as_ref().is_none_or(|b| b.is_empty());
            r.bars = bars;
            if r.bars.is_some() {
                // Con l'onda si trovano anche le frasi nel silenzio; le scelte già fatte restano valide perché
                // i punti già trovati non cambiano ordine (si aggiungono soltanto).
                let before: Vec<(Kind, Vec<usize>, String)> = r.flags.iter().map(|f| (f.kind, f.segs.clone(), f.snippet.clone())).collect();
                let dismissed: Vec<(Kind, Vec<usize>, String)> = r.dismissed.iter().filter_map(|i| before.get(*i).cloned()).collect();
                r.detect();
                r.dismissed.extend(r.flags.iter().enumerate().filter(|(_, f)| dismissed.iter().any(|(k, s, t)| *k == f.kind && *s == f.segs && *t == f.snippet)).map(|(i, _)| i));
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
        let ext = FORMATS[r.format.min(FORMATS.len() - 1)].1;
        let draft = r.draft_line();
        let (name, bytes) = r.export(ext);
        let dir = dest.or_else(|| r.source.parent().filter(|p| !p.as_os_str().is_empty()).map(Path::to_path_buf)).unwrap_or_else(|| PathBuf::from("."));
        let mut out = dir.join(&name);
        let stem = Path::new(&name).file_stem().and_then(|s| s.to_str()).unwrap_or("trascrizione");
        let mut n = 2;
        while out.exists() {
            out = dir.join(format!("{stem} ({n}).{ext}"));
            n += 1;
        }
        match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&out, bytes)) {
            Ok(()) => {
                // Le frasi corrette restano: la prossima volta si riparte da qui.
                let _ = r.persist();
                let name = out.file_name().and_then(|n| n.to_str()).unwrap_or("");
                r.note = match r.open_count() {
                    n if draft.is_some() => format!("Salvato come bozza ({n} da riascoltare): {name}"),
                    _ => format!("Salvato: {name}"),
                };
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
        Segment { t0, t1: t0 + 500, text: text.to_string(), speaker: None }
    }

    fn sample() -> Review {
        let mut segments = vec![seg(0, "Buongiorno a tutti."), seg(600, "Il bilancio è stato approvato."), seg(1200, "Il bilancio è stato approvato."), seg(1800, "Il bilancio è stato approvato."), seg(2400, "Sì sì sì sì sì sì sì va bene.")];
        segments.push(seg(3000, "Grazie e arrivederci."));
        Review::new(Path::new("/tmp/prova.wav"), 40.0, "it".into(), segments, Vec::new(), None)
    }

    fn with_voices() -> Review {
        let segments: Vec<Segment> = [(0, 0, "Buonasera."), (600, 1, "Grazie."), (1200, 2, "Chiedo la parola."), (1800, 0, "Prego."), (2400, 2, "Ho finito.")]
            .iter()
            .map(|(t0, v, text)| Segment { t0: *t0, t1: t0 + 500, text: text.to_string(), speaker: Some(*v) })
            .collect();
        Review::new(Path::new("/tmp/voci.wav"), 30.0, "it".into(), segments, trascrizione::default_names(3), None)
    }

    #[test]
    fn voices_change_merge_and_undo() {
        let mut r = with_voices();
        assert_eq!(r.tab, 1, "con le voci e niente da controllare si apre «Chi parla»");
        assert_eq!(r.live_voices(), [0, 1, 2]);
        assert_eq!(r.voice_stats(2), (2, 10.0));
        r.push_undo();
        r.names[0] = "Sindaco".into();
        r.merge(2, 1);
        assert_eq!(r.live_voices(), [0, 1]);
        assert_eq!(r.segments().iter().map(|s| s.speaker).collect::<Vec<_>>(), [Some(0), Some(1), Some(1), Some(0), Some(1)]);
        // Una voce nuova: la quarta, con il nome di partenza.
        r.set_speaker(4, crate::ui::core::NEW_VOICE);
        assert_eq!((r.rows[4].speaker, r.name(3).as_str()), (Some(3), "Interlocutore 4"));
        assert_eq!(r.name(0), "Sindaco");
        r.undo();
        assert_eq!(r.live_voices(), [0, 1, 2]);
        assert_eq!((r.name(0).as_str(), r.names.len()), ("Interlocutore 1", 3));
        assert_eq!(r.voice_sample(2), Some(2));
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

    #[test]
    fn a_report_stays_a_draft_until_every_point_is_checked() {
        let segments = vec![seg(0, "Cane meticcio di otto anni."), seg(600, "Massa di 12 mm al fegato."), seg(1200, "Non si osservano metastasi."), seg(1800, "Buona giornata.")];
        let mut r = Review::new(Path::new("/tmp/referto.wav"), 30.0, "it".into(), segments, Vec::new(), None);
        assert_eq!(r.open_count(), 0, "senza referto i numeri non si segnano");
        r.referto = true;
        r.detect();
        let kinds: Vec<(Kind, usize)> = r.flags.iter().map(|f| (f.kind, f.segs[0])).collect();
        assert_eq!(kinds, [(Kind::Number, 0), (Kind::Number, 1), (Kind::Meaning, 2)]);
        let line = r.draft_line().expect("ci sono punti aperti: è una bozza");
        assert!(line.starts_with("BOZZA — 3 punti") && line.contains("«12 mm»") && line.contains("«non» «metastasi»"), "{line}");
        // Confermata la prima, corretta a mano la seconda: resta la terza.
        r.fix(0);
        r.rows[1].text = "Massa di 15 mm al fegato.".into();
        assert_eq!(r.draft_points(), [2]);
        r.dismissed.insert(2);
        assert!(!r.is_draft() && r.draft_line().is_none());
    }

    #[test]
    fn report_confirmations_survive_reopen_without_confirming_other_sentences() {
        let path=std::env::temp_dir().join(format!("cedro-referto-{}-{}.wav",std::process::id(),std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos()));
        let mut r=Review::new(&path,20.0,"it".into(),vec![seg(0,"Cane di otto anni. Massa di 2.5 cm. La lesione non è capsulata.")],vec![],None);
        r.referto=true;r.view=2;r.detect();assert_eq!(r.open_count(),3);
        r.fix(0);r.persist().unwrap();
        let mut r=Review::from_saved(&path,revisione::load(&path).unwrap());
        assert!(r.referto);assert_eq!(r.view,2);assert_eq!(r.open_count(),2);
        r.bars=Some(vec![0.5;600]);r.detect();assert_eq!(r.open_count(),2,"l'onda non perde le conferme");
        r.rows[0].text="Cane di otto anni. Massa di 3.5 cm. La lesione non è capsulata.".into();
        assert_eq!(r.open_count(),1);r.persist().unwrap();
        let mut r=Review::from_saved(&path,revisione::load(&path).unwrap());
        assert_eq!(r.open_count(),1,"una correzione non approva altre frasi nello stesso segmento");
        let f=r.flag_of(0).unwrap();assert_eq!(r.flags[f].kind,Kind::Meaning);r.fix(f);r.persist().unwrap();
        assert!(!Review::from_saved(&path,revisione::load(&path).unwrap()).is_draft());
        revisione::store_summary(&path,&crate::riassunto::Stored::default()).unwrap();
        assert!(!Review::from_saved(&path,revisione::load(&path).unwrap()).is_draft(),"il riassunto conserva le conferme");
        std::fs::remove_file(revisione::review_path(&path)).unwrap();
    }

    #[test]
    fn every_export_keeps_draft_name_and_warning_until_confirmed() {
        use std::io::Read;
        let mut r=Review::new(Path::new("referto.wav"),20.0,"it".into(),vec![seg(0,"Tre mitosi in due punto cinque centimetri, non si osserva atipia.")],vec![],None);
        r.referto=true;r.detect();assert!(r.is_draft());
        for ext in ["txt","docx","srt","vtt"] {
            let (name,bytes)=r.export(ext);assert_eq!(name,format!("referto - bozza.{ext}"));
            let text=if ext=="docx" {
                let mut z=zip::ZipArchive::new(std::io::Cursor::new(bytes)).unwrap();let mut text=String::new();
                z.by_name("word/document.xml").unwrap().read_to_string(&mut text).unwrap();text
            }else{String::from_utf8(bytes).unwrap()};
            assert!(text.contains("BOZZA"),"{ext}: {text}");assert!(text.contains("mitosi"));
        }
        for i in 0..r.flags.len(){r.fix(i);}
        for ext in ["txt","docx","srt","vtt"]{assert_eq!(r.export(ext).0,format!("referto - rivisto.{ext}"));}
    }

    #[test]
    fn a_wave_that_cannot_be_read_stops_waiting() {
        let mut app = App::new(std::sync::Arc::new(|| {}));
        app.review = Some(sample());
        assert!(!app.review.as_ref().unwrap().wave_failed);
        app.review_wave(Path::new("/tmp/prova.wav"), None);
        assert!(app.review.as_ref().unwrap().wave_failed, "l'onda resterebbe «in lettura» per sempre");
        app.review_wave(Path::new("/tmp/prova.wav"), Some(vec![0.5; 10]));
        assert!(!app.review.as_ref().unwrap().wave_failed);
    }
}
