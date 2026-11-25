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
    /// Lo strumento da cui vengono i dati («Tecan Magellan · Sunrise · Assorbanza · 24/11/2026 14:05»); vuoto se non è un lavoro sui dati di uno strumento.
    pub strumento: String,
    /// I file originali letti: nome e impronta SHA-256, per sapere da dove viene il dato.
    pub origini: Vec<(String, String)>,
}

impl Lavoro {
    /// Il testo da incollare nel quaderno: il caso, lo strumento e l'originale con la sua impronta, poi un nome di file per riga.
    pub fn per_quaderno(&self) -> String {
        let mut t = String::new();
        if !self.caso.is_empty() {
            t.push_str(&format!("Caso {}\n", self.caso));
        }
        if !self.strumento.is_empty() {
            t.push_str(&self.strumento);
            t.push('\n');
        }
        for (nome, sha) in &self.origini {
            t.push_str(&format!("{} {nome} (SHA-256 {sha})\n", crate::i18n::tr("Originale:")));
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
/// Separa le coppie nome-impronta dei file originali.
const SEP2: char = '\u{1e}';

fn riga(l: &Lavoro) -> String {
    let pulito = |s: &str| s.replace(['\t', '\n', '\r', SEP, SEP2], " ");
    let outs: Vec<String> = l.outputs.iter().map(|o| pulito(o)).collect();
    let orig: Vec<String> = l.origini.iter().map(|(n, h)| format!("{}{SEP}{}", pulito(n), pulito(h))).collect();
    // I campi dopo i file sono venuti dopo: una riga di prima ne ha quattro e si legge lo stesso.
    format!("{}\t{}\t{}\t{}\t{}\t{}", l.when, pulito(&l.what), pulito(&l.caso), outs.join(&SEP.to_string()), pulito(&l.strumento), orig.join(&SEP2.to_string()))
}

fn leggi_riga(line: &str) -> Option<Lavoro> {
    let f: Vec<&str> = line.splitn(6, '\t').collect();
    if f.len() < 4 {
        return None;
    }
    let origini = f.get(5).map(|o| o.split(SEP2).filter_map(|c| c.split_once(SEP)).map(|(n, h)| (n.to_string(), h.to_string())).collect()).unwrap_or_default();
    Some(Lavoro { when: f[0].parse().ok()?, what: f[1].to_string(), caso: f[2].to_string(), outputs: f[3].split(SEP).filter(|s| !s.is_empty()).map(str::to_string).collect(), strumento: f.get(4).copied().unwrap_or("").to_string(), origini })
}

/// Quanti lavori si ricordano al più: abbastanza per il registro di un anno di un laboratorio.
const MAX: usize = 5000;

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

/// Il registro dei lavori in Excel: una riga per file uscito (data, ora, attività, caso, nota del caso
/// se è quello attivo, file), con i filtri accesi e la prima riga ferma, da allegare a una pratica o a
/// un controllo di qualità. Il caso si filtra dal menu della colonna.
pub fn registro_excel(elenco: &[Lavoro], caso_attivo: Option<&Caso>) -> Result<Vec<u8>, String> {
    use rust_xlsxwriter::{Color, Format, FormatBorder, Workbook};
    let err = |e: rust_xlsxwriter::XlsxError| e.to_string();
    let mut book = Workbook::new();
    let ws = book.add_worksheet();
    ws.set_name("Registro").map_err(err)?;
    let titoli = Format::new().set_bold().set_background_color(Color::RGB(0xE3F1E6)).set_border_bottom(FormatBorder::Thin);
    let testa = ["Data", "Ora", "Attività", "Caso", "Nota del caso", "File", "Strumento", "File originale (SHA-256)"];
    for (c, t) in testa.iter().enumerate() {
        ws.write_string_with_format(0, c as u16, crate::i18n::tr(t), &titoli).map_err(err)?;
    }
    let mut r = 1u32;
    for l in elenco {
        let nota = caso_attivo.filter(|c| c.code == l.caso).map(|c| c.note.as_str()).unwrap_or("");
        let files: Vec<&str> = if l.outputs.is_empty() { vec![""] } else { l.outputs.iter().map(String::as_str).collect() };
        for f in files {
            ws.write_string(r, 0, l.giorno()).map_err(err)?;
            ws.write_string(r, 1, l.ora()).map_err(err)?;
            ws.write_string(r, 2, &l.what).map_err(err)?;
            ws.write_string(r, 3, &l.caso).map_err(err)?;
            ws.write_string(r, 4, nota).map_err(err)?;
            ws.write_string(r, 5, f).map_err(err)?;
            ws.write_string(r, 6, &l.strumento).map_err(err)?;
            let orig: Vec<String> = l.origini.iter().map(|(n, h)| format!("{n} · {h}")).collect();
            ws.write_string(r, 7, orig.join(" | ")).map_err(err)?;
            r += 1;
        }
    }
    for (c, w) in [12.0, 7.0, 26.0, 18.0, 24.0, 60.0, 44.0, 70.0].iter().enumerate() {
        ws.set_column_width(c as u16, *w).map_err(err)?;
    }
    ws.set_freeze_panes(1, 0).map_err(err)?;
    ws.autofilter(0, 0, r.saturating_sub(1).max(1), 7).map_err(err)?;
    ws.set_landscape();
    ws.set_print_fit_to_pages(1, 0);
    ws.set_header("&L&\"Calibri,Bold\"Cedro 4 Labs&C&A&R&D");
    ws.set_footer("&C&P / &N");
    book.save_to_buffer().map_err(err)
}

/// Salva il registro in `dir` come «Registro lavori AAAA-MM-GG.xlsx» (con un numero se esiste già).
pub fn salva_registro(dir: &std::path::Path, elenco: &[Lavoro], caso_attivo: Option<&Caso>) -> Result<PathBuf, String> {
    let bytes = registro_excel(elenco, caso_attivo)?;
    let s = crate::nametemplate::now();
    let base = format!("{} {:04}-{:02}-{:02}", crate::i18n::tr("Registro lavori"), s.year, s.month, s.day);
    let mut out = dir.join(format!("{base}.xlsx"));
    let mut n = 2;
    while out.exists() {
        out = dir.join(format!("{base} ({n}).xlsx"));
        n += 1;
    }
    std::fs::write(&out, bytes).map_err(|e| e.to_string())?;
    Ok(out)
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
    fn il_registro_ha_una_riga_per_file_con_i_filtri() {
        let elenco = vec![
            Lavoro { when: 1_793_196_309, what: "Togli i nomi".into(), caso: "V-1".into(), outputs: vec!["a.svs".into(), "b.svs".into()], strumento: String::new(), origini: Vec::new() },
            Lavoro { when: 1_793_199_909, what: "Prepara una figura".into(), caso: String::new(), outputs: vec!["fig.tif".into()], strumento: String::new(), origini: Vec::new() },
        ];
        let caso = Caso::new("V-1", "Luna, labrador").unwrap();
        let x = registro_excel(&elenco, Some(&caso)).unwrap();
        let fogli = crate::tabella::leggi("xlsx", &x).unwrap();
        let r = &fogli[0].righe;
        assert_eq!(r.len(), 4);
        assert_eq!(r[1][3], crate::tabella::Cella::Testo("V-1".into()));
        assert_eq!(r[1][4], crate::tabella::Cella::Testo("Luna, labrador".into()));
        assert_eq!(r[2][5], crate::tabella::Cella::Testo("b.svs".into()));
        assert_eq!(r[3][2], crate::tabella::Cella::Testo("Prepara una figura".into()));
        assert_eq!(r[3][4], crate::tabella::Cella::Vuota);
        let dir = std::env::temp_dir().join(format!("cedro-registro-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let a = salva_registro(&dir, &elenco, None).unwrap();
        let b = salva_registro(&dir, &elenco, None).unwrap();
        assert_ne!(a, b);
        assert!(b.to_string_lossy().ends_with("(2).xlsx"));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn jobs_round_trip_through_a_line() {
        let l = Lavoro { when: 1_793_196_309, what: "Togli i nomi".into(), caso: "V-1".into(), outputs: vec!["a b.svs".into(), "c\td.svs".into()], strumento: String::new(), origini: Vec::new() };
        let back = leggi_riga(&riga(&l)).unwrap();
        assert_eq!(back.what, "Togli i nomi");
        assert_eq!(back.outputs, vec!["a b.svs".to_string(), "c d.svs".to_string()]);
        assert!(leggi_riga("no tabs").is_none());
        assert_eq!(l.per_quaderno(), "Caso V-1\na b.svs\nc\td.svs");
        let senza = Lavoro { caso: String::new(), ..l };
        assert_eq!(senza.per_quaderno(), "a b.svs\nc\td.svs");
    }
}
