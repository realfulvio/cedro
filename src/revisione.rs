//! Rivedi trascrizione: le frasi di una trascrizione finita restano salvate per riaprirle, un
//! rilevatore segnala i difetti tipici di whisper (frasi ripetute in giro, frasi inventate nel
//! silenzio) e la revisione si esporta anche come sottotitoli .srt e .vtt.

use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::trascrizione::{self, Segment};

// ---------------------------------------------------------------- frasi salvate

/// Quel che serve per riaprire la revisione di un file.
pub struct Saved {
    pub secs: f64,
    pub lang: String,
    pub segments: Vec<Segment>,
}

fn folder() -> PathBuf {
    trascrizione::data_dir().join("revisioni")
}

/// Dove sta la revisione dell'audio `source`: un nome dal percorso, così due file con lo stesso nome non si confondono.
pub fn review_path(source: &Path) -> PathBuf {
    let canonical = std::fs::canonicalize(source).unwrap_or_else(|_| source.to_path_buf());
    let digest = Sha256::digest(canonical.to_string_lossy().as_bytes());
    let name: String = digest.iter().take(10).map(|b| format!("{b:02x}")).collect();
    folder().join(format!("{name}.json"))
}

pub fn has(source: &Path) -> bool {
    review_path(source).is_file()
}

pub fn save(source: &Path, secs: f64, lang: &str, segments: &[Segment]) -> std::io::Result<()> {
    let list: Vec<Value> = segments.iter().map(|s| json!([s.t0, s.t1, s.text])).collect();
    let body = json!({"source": source.to_string_lossy(), "secs": secs, "lang": lang, "segments": list});
    let path = review_path(source);
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Si scrive in un file accanto e poi si rinomina: una revisione a metà non si vede mai.
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, body.to_string())?;
    std::fs::rename(&temp, &path)
}

pub fn load(source: &Path) -> Option<Saved> {
    let text = std::fs::read_to_string(review_path(source)).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let segments: Vec<Segment> = v["segments"]
        .as_array()?
        .iter()
        .filter_map(|s| Some(Segment { t0: s[0].as_i64()?, t1: s[1].as_i64()?, text: s[2].as_str()?.to_string() }))
        .collect();
    (!segments.is_empty()).then(|| Saved { secs: v["secs"].as_f64().unwrap_or(0.0), lang: v["lang"].as_str().unwrap_or("it").to_string(), segments })
}

// ---------------------------------------------------------------- rilevatore

/// Che difetto è.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// La stessa frase una dietro l'altra.
    Repeat,
    /// Un giro di parole ripetuto dentro la stessa frase.
    Loop,
    /// Una frase che whisper inventa («Sottotitoli a cura di…»).
    Typical,
    /// Una frase scritta dove l'audio è quasi muto.
    Silence,
}

/// Un punto da controllare.
#[derive(Clone, Debug)]
pub struct Flag {
    pub kind: Kind,
    /// Le frasi coinvolte (indici); per `Repeat` la prima è quella che si tiene.
    pub segs: Vec<usize>,
    /// Quante volte si ripete.
    pub count: usize,
}

impl Flag {
    /// Il tempo (centesimi) da cui far partire l'ascolto.
    pub fn start(&self, rows: &[Segment]) -> i64 {
        self.segs.first().and_then(|i| rows.get(*i)).map_or(0, |s| s.t0)
    }
}

/// Parole in minuscolo, senza punteggiatura.
fn words(text: &str) -> Vec<String> {
    text.split_whitespace()
        .map(|w| w.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect::<String>())
        .filter(|w| !w.is_empty())
        .collect()
}

/// Due frasi uguali o quasi: stesse parole almeno al 85%.
fn similar(a: &[String], b: &[String]) -> bool {
    if a.is_empty() || b.is_empty() {
        return false;
    }
    if a == b {
        return true;
    }
    let (short, long) = if a.len() <= b.len() { (a, b) } else { (b, a) };
    if long.len() > short.len() + short.len() / 4 + 1 {
        return false;
    }
    let mut pool: Vec<&String> = long.iter().collect();
    let hits = short
        .iter()
        .filter(|w| match pool.iter().position(|p| p == w) {
            Some(i) => {
                pool.swap_remove(i);
                true
            }
            None => false,
        })
        .count();
    hits * 100 >= long.len() * 85
}

/// Se dentro la frase un giro di parole si ripete di fila (almeno 3 volte, 4 se è una parola sola),
/// lo riduce a una volta sola. Dà il testo nuovo e quante volte si ripeteva.
pub fn collapse_loops(text: &str) -> Option<(String, usize)> {
    let mut parts: Vec<String> = text.split_whitespace().map(str::to_string).collect();
    let mut worst = 0;
    'again: loop {
        let norm: Vec<String> = parts.iter().map(|w| w.chars().filter(|c| c.is_alphanumeric()).flat_map(char::to_lowercase).collect()).collect();
        for n in 1..=12usize {
            if norm.len() < n * 3 {
                break;
            }
            let mut i = 0;
            while i + n <= norm.len() {
                if norm[i..i + n].iter().all(String::is_empty) {
                    i += 1;
                    continue;
                }
                let mut reps = 1;
                while i + (reps + 1) * n <= norm.len() && norm[i + reps * n..i + (reps + 1) * n] == norm[i..i + n] {
                    reps += 1;
                }
                if reps >= if n == 1 { 4 } else { 3 } {
                    // Resta la prima volta; la punteggiatura che chiude l'ultima si porta sull'ultima parola rimasta.
                    let tail: String = parts[i + reps * n - 1].chars().rev().take_while(|c| !c.is_alphanumeric()).collect::<Vec<_>>().into_iter().rev().collect();
                    let keep_end = i + n - 1;
                    let stem: String = parts[keep_end].trim_end_matches(|c: char| !c.is_alphanumeric()).to_string();
                    parts[keep_end] = format!("{stem}{tail}");
                    parts.drain(i + n..i + reps * n);
                    worst = worst.max(reps);
                    continue 'again;
                }
                i += 1;
            }
        }
        break;
    }
    (worst > 0).then(|| (parts.join(" "), worst))
}

/// Frasi che whisper scrive da sé dove non c'è parlato (il testo, già in minuscolo e senza segni).
const TYPICAL: &[&str] = &[
    "sottotitoli a cura di",
    "sottotitoli creati dalla comunità",
    "sottotitoli e revisione a cura di",
    "sottotitoli e revisione",
    "traduzione a cura di",
    "grazie per la visione",
    "grazie per aver guardato",
    "grazie per l ascolto",
    "iscriviti al canale",
    "amara org",
    "subtitles by",
    "subtitles by the amara org community",
    "thanks for watching",
    "thank you for watching",
    "please subscribe",
    "sous titres",
    "untertitel der amara org community",
    "subtítulos realizados por la comunidad de amara org",
    "qtss",
];

fn is_typical(text: &str) -> bool {
    let norm = text.split_whitespace().map(|w| w.chars().map(|c| if c.is_alphanumeric() { c } else { ' ' }).collect::<String>()).collect::<Vec<_>>().join(" ").to_lowercase();
    let norm = norm.split_whitespace().collect::<Vec<_>>().join(" ");
    // Una frase lunga che ne contiene una tipica è probabilmente vera.
    norm.split(' ').count() <= 14 && TYPICAL.iter().any(|t| norm.contains(t))
}

/// L'onda dell'audio: l'intensità (0..1) in colonne uguali su tutta la durata.
pub struct Wave<'a> {
    pub bars: &'a [f32],
    pub secs: f64,
}

impl Wave<'_> {
    /// L'intensità media tra due istanti (centesimi di secondo).
    pub fn energy(&self, t0: i64, t1: i64) -> f32 {
        if self.bars.is_empty() || self.secs <= 0.0 {
            return 1.0;
        }
        let n = self.bars.len();
        let col = |t: i64| (((t as f64 / 100.0) / self.secs) * n as f64).clamp(0.0, n as f64) as usize;
        let (a, b) = (col(t0), col(t1).max(col(t0) + 1).min(n));
        let slice = &self.bars[a.min(n - 1)..b.max(a.min(n - 1) + 1)];
        slice.iter().sum::<f32>() / slice.len() as f32
    }
}

fn median(mut v: Vec<f32>) -> f32 {
    v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    v.get(v.len() / 2).copied().unwrap_or(0.0)
}

/// Trova i punti da controllare, nell'ordine in cui compaiono.
pub fn find(rows: &[Segment], wave: Option<&Wave>) -> Vec<Flag> {
    let mut flags: Vec<Flag> = Vec::new();
    let norm: Vec<Vec<String>> = rows.iter().map(|r| words(&r.text)).collect();

    // Una frase uguale a quella prima, tre volte di fila o più (due, se è lunga).
    let mut i = 0;
    while i < rows.len() {
        let mut j = i + 1;
        while j < rows.len() && similar(&norm[i], &norm[j]) {
            j += 1;
        }
        let run = j - i;
        let long = norm[i].len() >= 6;
        if (run >= 3 && (norm[i].len() >= 2 || run >= 4)) || (run == 2 && long) {
            flags.push(Flag { kind: Kind::Repeat, segs: (i..j).collect(), count: run });
            i = j;
        } else {
            i += 1;
        }
    }
    // Un giro di parole che si ripete dentro la frase.
    for (i, r) in rows.iter().enumerate() {
        if let Some((_, count)) = collapse_loops(&r.text) {
            flags.push(Flag { kind: Kind::Loop, segs: vec![i], count });
        }
    }
    // Frasi inventate, e frasi dove l'audio è quasi muto.
    let quiet_limit = wave.map(|w| {
        let e: Vec<f32> = rows.iter().filter(|r| !is_typical(&r.text)).map(|r| w.energy(r.t0, r.t1)).collect();
        median(e) * 0.08
    });
    for (i, r) in rows.iter().enumerate() {
        let quiet = match (wave, quiet_limit) {
            (Some(w), Some(limit)) => r.t1 - r.t0 >= 150 && w.energy(r.t0, r.t1) < limit,
            _ => false,
        };
        let typical = is_typical(&r.text);
        if typical || quiet {
            let already = flags.iter().any(|f| f.kind == Kind::Repeat && f.segs.contains(&i));
            if !already {
                flags.push(Flag { kind: if quiet { Kind::Silence } else { Kind::Typical }, segs: vec![i], count: 1 });
            }
        }
    }
    flags.sort_by_key(|f| f.segs.first().copied().unwrap_or(0));
    flags
}

// ---------------------------------------------------------------- sottotitoli

fn stamp(centis: i64, comma: bool) -> String {
    let c = centis.max(0);
    let (h, m, s, ms) = (c / 360_000, c / 6000 % 60, c / 100 % 60, c % 100 * 10);
    format!("{h:02}:{m:02}:{s:02}{}{ms:03}", if comma { ',' } else { '.' })
}

/// Una frase lunga sulla riga dei sottotitoli va a capo a metà, vicino a uno spazio.
fn wrap(text: &str) -> String {
    let t = text.trim();
    if t.chars().count() <= 42 {
        return t.to_string();
    }
    let mid = t.chars().count() / 2;
    let mut best: Option<usize> = None;
    for (n, (at, c)) in t.char_indices().enumerate() {
        if c == ' ' && best.is_none_or(|b| n.abs_diff(mid) < t[..b].chars().count().abs_diff(mid)) {
            best = Some(at);
        }
    }
    match best {
        Some(at) => format!("{}\n{}", t[..at].trim_end(), t[at..].trim_start()),
        None => t.to_string(),
    }
}

fn cues(segments: &[Segment]) -> Vec<(i64, i64, String)> {
    let live: Vec<&Segment> = segments.iter().filter(|s| !s.text.trim().is_empty()).collect();
    live.iter()
        .enumerate()
        .map(|(k, s)| {
            // Un sottotitolo non deve finire dopo l'inizio del successivo, né durare meno di mezzo secondo.
            let end = s.t1.max(s.t0 + 50).min(live.get(k + 1).map_or(i64::MAX, |n| n.t0.max(s.t0 + 1)));
            (s.t0, end, wrap(&s.text))
        })
        .collect()
}

pub fn srt_bytes(segments: &[Segment]) -> Vec<u8> {
    let mut out = String::new();
    for (n, (a, b, text)) in cues(segments).iter().enumerate() {
        out.push_str(&format!("{}\n{} --> {}\n{text}\n\n", n + 1, stamp(*a, true), stamp(*b, true)));
    }
    out.into_bytes()
}

pub fn vtt_bytes(segments: &[Segment]) -> Vec<u8> {
    let mut out = String::from("WEBVTT\n\n");
    for (a, b, text) in cues(segments) {
        out.push_str(&format!("{} --> {}\n{text}\n\n", stamp(a, false), stamp(b, false)));
    }
    out.into_bytes()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seg(t0: i64, t1: i64, text: &str) -> Segment {
        Segment { t0, t1, text: text.to_string() }
    }

    #[test]
    fn saved_reviews_survive_and_stay_apart() {
        let dir = std::env::temp_dir().join(format!("cedro-rev-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::env::set_var("XDG_DATA_HOME", &dir);
        let a = dir.join("giunta.m4a");
        let b = dir.join("altra").join("giunta.m4a");
        assert_ne!(review_path(&a), review_path(&b));
        assert!(!has(&a));
        save(&a, 12.5, "it", &[seg(0, 300, "Buongiorno a tutti."), seg(300, 700, "Iniziamo \"subito\" con l'ordine del giorno.")]).unwrap();
        assert!(has(&a) && !has(&b));
        let back = load(&a).unwrap();
        assert_eq!((back.secs, back.lang.as_str(), back.segments.len()), (12.5, "it", 2));
        assert_eq!(back.segments[1].text, "Iniziamo \"subito\" con l'ordine del giorno.");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn repeated_sentences_are_found_and_normal_speech_is_not() {
        let mut rows = vec![seg(0, 300, "Se non ci sono altre domande votiamo il punto tre.")];
        for k in 0..6 {
            rows.push(seg(300 + k * 300, 600 + k * 300, if k % 2 == 0 { "Allora procediamo." } else { "allora, procediamo" }));
        }
        rows.push(seg(2100, 2400, "Favorevoli? Unanimità."));
        let flags = find(&rows, None);
        assert_eq!(flags.len(), 1, "{flags:?}");
        assert_eq!((flags[0].kind, flags[0].segs.clone(), flags[0].count), (Kind::Repeat, (1..7).collect(), 6));

        // Due frasi corte uguali, o tre «sì» di fila, sono parlato normale.
        let normal = vec![seg(0, 100, "Sì."), seg(100, 200, "Sì."), seg(200, 300, "Sì."), seg(300, 400, "Va bene."), seg(400, 500, "Va bene.")];
        assert!(find(&normal, None).is_empty());
        // Frasi diverse che cominciano allo stesso modo non sono una ripetizione.
        let alike = vec![seg(0, 100, "Il punto uno riguarda il bilancio."), seg(100, 200, "Il punto due riguarda la scuola."), seg(200, 300, "Il punto tre riguarda le strade.")];
        assert!(find(&alike, None).is_empty());
    }

    #[test]
    fn loops_inside_a_sentence_collapse_to_one() {
        let (text, n) = collapse_loops("Allora procediamo, allora procediamo, allora procediamo…").unwrap();
        assert_eq!((text.as_str(), n), ("Allora procediamo…", 3));
        let (text, n) = collapse_loops("Il costo è di no no no no no mila euro.").unwrap();
        assert_eq!((text.as_str(), n), ("Il costo è di no mila euro.", 5));
        assert!(collapse_loops("Sì sì sì va bene così, grazie grazie.").is_none());
        assert!(collapse_loops("Un giro due giri tre giri.").is_none());
        let rows = vec![seg(0, 500, "Ok ok ok ok ok ok ok ok ok.")];
        assert_eq!(find(&rows, None)[0].kind, Kind::Loop);
    }

    #[test]
    fn typical_whisper_phrases_and_silence_are_flagged() {
        let rows = vec![
            seg(0, 400, "Buongiorno a tutti e benvenuti alla seduta di giunta."),
            seg(400, 800, "Il primo punto riguarda il bilancio di previsione."),
            seg(800, 1200, "Sottotitoli a cura di QTSS"),
            seg(1200, 1700, "Procediamo con il voto del secondo punto."),
            seg(1700, 2100, "Grazie per la visione!"),
        ];
        let flags = find(&rows, None);
        assert_eq!(flags.iter().map(|f| (f.kind, f.segs[0])).collect::<Vec<_>>(), vec![(Kind::Typical, 2), (Kind::Typical, 4)]);
        // Con l'onda: tutto suona tranne un tratto muto, dove una frase qualunque è sospetta.
        let mut bars = vec![0.5f32; 21];
        bars[12..17].fill(0.002);
        let wave = Wave { bars: &bars, secs: 21.0 };
        let mut rows = rows;
        rows[3].text = "Una frase perfettamente normale ma scritta sul nulla.".into();
        let flags = find(&rows, Some(&wave));
        let silent: Vec<usize> = flags.iter().filter(|f| f.kind == Kind::Silence).map(|f| f.segs[0]).collect();
        assert_eq!(silent, vec![3], "{flags:?}");
    }

    #[test]
    fn subtitles_have_the_right_clock_and_never_overlap() {
        let rows = vec![seg(0, 450, "Buongiorno a tutti."), seg(400, 5000, "Questa è una frase abbastanza lunga da dover andare a capo sulla riga."), seg(5000, 5010, "Fine."), seg(6000, 7000, "  ")];
        let srt = String::from_utf8(srt_bytes(&rows)).unwrap();
        assert!(srt.starts_with("1\n00:00:00,000 --> 00:00:04,000\nBuongiorno a tutti.\n\n2\n00:00:04,000 --> 00:00:50,000\n"), "{srt}");
        assert!(srt.contains("abbastanza lunga\nda dover andare a capo sulla riga."), "{srt}");
        assert!(srt.contains("3\n00:00:50,000 --> 00:00:50,500\nFine."), "{srt}");
        assert!(!srt.contains("4\n"), "le frasi vuote non diventano sottotitoli");
        let vtt = String::from_utf8(vtt_bytes(&rows)).unwrap();
        assert!(vtt.starts_with("WEBVTT\n\n00:00:00.000 --> 00:00:04.000\n"), "{vtt}");
        assert_eq!(stamp(372_550, true), "01:02:05,500");
    }
}
