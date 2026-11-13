//! Il caso o campione su cui si lavora, e il registro dei lavori fatti.
//!
//! In laboratorio ogni vetrino, radiografia o trascrizione appartiene a un caso («V-2026-0457»). Se Cedro
//! sa qual è, lo mette nei nomi dei file che scrive (`%caso%`, vedi `nametemplate`) e tiene l'elenco di
//! quello che è uscito, da copiare nel quaderno di laboratorio. Tutto resta sul computer, in due
//! file di testo accanto alle impostazioni.

use std::path::PathBuf;

/// Il modello dei nomi quando c'è un caso: «2026-11-13_V-2026-0457_vetrino».
pub const MODELLO: &str = "%data%_%caso%_%nome%";

/// Il caso scelto: un codice (entra nei nomi dei file) e una nota libera («Luna, labrador»).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Caso {
    pub code: String,
    pub note: String,
}

impl Caso {
    /// Un caso dal codice e dalla nota digitati; `None` se il codice è vuoto.
    /// Il codice diventa un pezzo di nome di file: niente caratteri che i file system non accettano.
    pub fn new(code: &str, note: &str) -> Option<Caso> {
        let code = pulisci(code);
        if code.is_empty() {
            return None;
        }
        Some(Caso { code, note: note.split_whitespace().collect::<Vec<_>>().join(" ") })
    }
}

/// Le opzioni di un lavoro con il caso: con un codice i nomi nuovi diventano «data_caso_nome»
/// (a meno di un modello già scelto); senza, restano quelle di prima.
pub fn opzioni(caso: Option<&str>, mut opts: crate::engine::Options) -> crate::engine::Options {
    if let Some(code) = caso {
        opts.caso = Some(code.to_string());
        if opts.name_template.is_none() {
            opts.name_template = Some(MODELLO.to_string());
        }
    }
    opts
}

/// Il codice come pezzo di nome: spazi e caratteri vietati diventano «-», al più 48 caratteri.
pub fn pulisci(code: &str) -> String {
    let mut out = String::new();
    for c in code.trim().chars() {
        let c = if c.is_control() || "\\/:*?\"<>|".contains(c) || c.is_whitespace() { '-' } else { c };
        if c == '-' && out.ends_with('-') {
            continue;
        }
        out.push(c);
        if out.chars().count() >= 48 {
            break;
        }
    }
    out.trim_matches(|c| c == '-' || c == '.' || c == '_').to_string()
}

fn dir() -> PathBuf {
    crate::engine::config_dir().join("lab")
}

fn caso_file() -> PathBuf {
    dir().join("caso.txt")
}

fn lavori_file() -> PathBuf {
    dir().join("lavori.txt")
}

fn scrivi(path: &PathBuf, text: &str) {
    if let Some(d) = path.parent() {
        let _ = std::fs::create_dir_all(d);
    }
    let _ = std::fs::write(path, text);
}

/// Il caso rimasto dall'ultima volta, se c'è.
pub fn carica() -> Option<Caso> {
    let text = std::fs::read_to_string(caso_file()).ok()?;
    let mut lines = text.lines();
    let code = lines.next()?;
    Caso::new(code, lines.next().unwrap_or(""))
}

/// Si ricorda il caso (o, con `None`, che non c'è più).
pub fn salva(caso: Option<&Caso>) {
    match caso {
        Some(c) => scrivi(&caso_file(), &format!("{}\n{}\n", c.code, c.note)),
        None => {
            let _ = std::fs::remove_file(caso_file());
        }
    }
}

/// Un lavoro fatto: cosa, quando, per quale caso, e i file usciti.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Lavoro {
    /// Secondi dal 1970.
    pub when: u64,
    /// Cosa si è fatto («Togli i nomi», «Salva immagini e vetrini»…).
    pub what: String,
    /// Il codice del caso di allora; vuoto se non ce n'era uno.
    pub caso: String,
    /// I nomi dei file scritti.
    pub outputs: Vec<String>,
}

impl Lavoro {
    /// Il testo da incollare nel quaderno: il caso, poi un nome di file per riga.
    pub fn per_quaderno(&self) -> String {
        let mut t = String::new();
        if !self.caso.is_empty() {
            t.push_str(&format!("Caso {}\n", self.caso));
        }
        t.push_str(&self.outputs.join("\n"));
        t
    }

    /// L'ora, «14:05».
    pub fn ora(&self) -> String {
        let s = crate::nametemplate::local(self.when);
        format!("{:02}:{:02}", s.hour, s.minute)
    }

    /// Il giorno, «2026-11-13», nel fuso del computer.
    pub fn giorno(&self) -> String {
        let s = crate::nametemplate::local(self.when);
        format!("{:04}-{:02}-{:02}", s.year, s.month, s.day)
    }
}

const SEP: char = '\u{1f}';

fn riga(l: &Lavoro) -> String {
    let pulito = |s: &str| s.replace(['\t', '\n', '\r', SEP], " ");
    let outs: Vec<String> = l.outputs.iter().map(|o| pulito(o)).collect();
    format!("{}\t{}\t{}\t{}", l.when, pulito(&l.what), pulito(&l.caso), outs.join(&SEP.to_string()))
}

fn leggi_riga(line: &str) -> Option<Lavoro> {
    let f: Vec<&str> = line.splitn(4, '\t').collect();
    if f.len() != 4 {
        return None;
    }
    Some(Lavoro { when: f[0].parse().ok()?, what: f[1].to_string(), caso: f[2].to_string(), outputs: f[3].split(SEP).filter(|s| !s.is_empty()).map(str::to_string).collect() })
}

/// Quanti lavori si ricordano al più: più vecchi di così non servono a un quaderno di tutti i giorni.
const MAX: usize = 400;

/// I lavori ricordati, dal più vecchio al più nuovo.
pub fn lavori() -> Vec<Lavoro> {
    let Ok(text) = std::fs::read_to_string(lavori_file()) else { return Vec::new() };
    text.lines().filter_map(leggi_riga).collect()
}

/// Aggiunge un lavoro all'elenco e lo ricorda.
pub fn registra(elenco: &mut Vec<Lavoro>, lavoro: Lavoro) {
    elenco.push(lavoro);
    if elenco.len() > MAX {
        let extra = elenco.len() - MAX;
        elenco.drain(..extra);
    }
    let text: String = elenco.iter().map(|l| riga(l) + "\n").collect();
    scrivi(&lavori_file(), &text);
}

/// I lavori di oggi (nel fuso del computer), dal più nuovo al più vecchio.
pub fn di_oggi(elenco: &[Lavoro]) -> Vec<&Lavoro> {
    let s = crate::nametemplate::now();
    let oggi = format!("{:04}-{:02}-{:02}", s.year, s.month, s.day);
    let mut v: Vec<&Lavoro> = elenco.iter().filter(|l| l.giorno() == oggi).collect();
    v.reverse();
    v
}

#[cfg(test)]
mod tests {
    #[test]
    fn le_opzioni_col_caso_cambiano_solo_i_nomi() {
        let senza = opzioni(None, crate::engine::Options::default());
        assert_eq!((senza.caso, senza.name_template), (None, None));
        let con = opzioni(Some("V-1"), crate::engine::Options::default());
        assert_eq!(con.caso.as_deref(), Some("V-1"));
        assert_eq!(con.name_template.as_deref(), Some(MODELLO));
        // Un modello scelto da chi usa Cedro resta com'è.
        let proprio = opzioni(Some("V-1"), crate::engine::Options { name_template: Some("%nome%_%caso%".into()), ..Default::default() });
        assert_eq!(proprio.name_template.as_deref(), Some("%nome%_%caso%"));
    }

    use super::*;

    #[test]
    fn the_code_becomes_a_safe_piece_of_name() {
        assert_eq!(Caso::new(" V-2026-0457 ", "Luna,  labrador ").unwrap(), Caso { code: "V-2026-0457".into(), note: "Luna, labrador".into() });
        assert_eq!(Caso::new("a/b: c*d", "").unwrap().code, "a-b-c-d");
        assert_eq!(pulisci("..__x__.."), "x");
        assert!(Caso::new("  /// ", "nota").is_none());
        assert!(Caso::new("", "").is_none());
        assert_eq!(pulisci(&"x".repeat(100)).chars().count(), 48);
    }

    #[test]
    fn jobs_round_trip_through_a_line() {
        let l = Lavoro { when: 1_793_196_309, what: "Togli i nomi".into(), caso: "V-1".into(), outputs: vec!["a b.svs".into(), "c\td.svs".into()] };
        let back = leggi_riga(&riga(&l)).unwrap();
        assert_eq!(back.what, "Togli i nomi");
        assert_eq!(back.outputs, vec!["a b.svs".to_string(), "c d.svs".to_string()]);
        assert!(leggi_riga("no tabs").is_none());
        assert_eq!(l.per_quaderno(), "Caso V-1\na b.svs\nc\td.svs");
        let senza = Lavoro { caso: String::new(), ..l };
        assert_eq!(senza.per_quaderno(), "a b.svs\nc\td.svs");
    }
}
