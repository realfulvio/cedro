//! Il file dello strumento come lo leggono i profili: il testo (CSV, TXT, XML) già decodificato e,
//! per Excel e Calc, i fogli come righe di testo.

use std::path::Path;

/// Un foglio di calcolo: nome e righe di celle come testo (numeri con il punto, date ISO).
#[derive(Clone, Debug)]
pub struct Foglio {
    pub nome: String,
    pub righe: Vec<Vec<String>>,
}

#[derive(Clone, Debug)]
pub struct Fonte {
    /// Il nome del file, senza cartella.
    pub nome: String,
    /// L'estensione in minuscolo.
    pub ext: String,
    pub bytes: Vec<u8>,
    /// Il testo, per i file di testo.
    pub testo: String,
    /// I fogli, per Excel e Calc.
    pub fogli: Vec<Foglio>,
    /// Le righe già divise (ogni profilo che prova a riconoscere il file le chiede).
    righe_pronte: std::sync::OnceLock<Vec<Vec<String>>>,
}

/// Quanto si legge al più: gli export degli strumenti sono di qualche MB.
const MAX: u64 = 256 << 20;

impl Fonte {
    pub fn apri(path: &Path) -> Result<Fonte, String> {
        let len = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
        if len > MAX {
            return Err("file troppo grande per l'export di uno strumento".into());
        }
        let bytes = crate::input_file::read(path).map_err(|e| e.to_string())?;
        let nome = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
        Ok(Fonte::da_bytes(&nome, bytes))
    }

    pub fn da_bytes(nome: &str, bytes: Vec<u8>) -> Fonte {
        let ext = Path::new(nome).extension().map(|e| e.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        let mut f = Fonte { nome: nome.to_string(), ext: ext.clone(), bytes, testo: String::new(), fogli: Vec::new(), righe_pronte: std::sync::OnceLock::new() };
        if matches!(ext.as_str(), "xlsx" | "xlsm" | "xls" | "ods") {
            if let Ok(fogli) = crate::tabella::leggi(&ext, &f.bytes) {
                f.fogli = fogli.into_iter().map(|s| Foglio { nome: s.nome, righe: s.righe.iter().map(|r| r.iter().map(cella).collect()).collect() }).collect();
            }
        } else {
            f.testo = crate::engine::decode_text(&f.bytes).replace("\r\n", "\n").replace('\r', "\n");
        }
        f
    }

    /// Il nome del file senza estensione.
    pub fn stem(&self) -> &str {
        Path::new(&self.nome).file_stem().and_then(|s| s.to_str()).unwrap_or(&self.nome)
    }

    pub fn sha256(&self) -> String {
        use sha2::Digest;
        sha2::Sha256::digest(&self.bytes).iter().map(|b| format!("{b:02x}")).collect()
    }

    /// È un foglio di calcolo.
    pub fn foglio(&self) -> bool {
        !self.fogli.is_empty()
    }

    /// Le righe del file: per un testo divise con il separatore più probabile, per un foglio di
    /// calcolo quelle del primo foglio.
    pub fn righe(&self) -> Vec<Vec<String>> {
        if let Some(f) = self.fogli.first() {
            return f.righe.clone();
        }
        self.righe_pronte
            .get_or_init(|| {
                let sep = crate::tabella::separatore(&self.testo, &self.ext);
                crate::textconv::parse_csv(&self.testo, sep)
            })
            .clone()
    }

    /// Le righe del testo divise con un separatore preciso.
    pub fn righe_con(&self, sep: char) -> Vec<Vec<String>> {
        if let Some(f) = self.fogli.first() {
            return f.righe.clone();
        }
        crate::textconv::parse_csv(&self.testo, sep)
    }

    /// L'inizio del file come testo, per riconoscerlo: per i fogli le prime righe unite da tabulazioni.
    pub fn inizio(&self, n: usize) -> String {
        if self.foglio() {
            let mut s = String::new();
            for f in &self.fogli {
                s.push_str(&f.nome);
                s.push('\n');
                for r in f.righe.iter().take(n) {
                    s.push_str(&r.join("\t"));
                    s.push('\n');
                }
            }
            return s;
        }
        self.testo.lines().take(n).collect::<Vec<_>>().join("\n")
    }

    /// Il foglio con questo nome (senza badare a maiuscole e spazi).
    pub fn foglio_nome(&self, nome: &str) -> Option<&Foglio> {
        let n = nome.trim().to_ascii_lowercase();
        self.fogli.iter().find(|f| f.nome.trim().to_ascii_lowercase() == n)
    }
}

/// La cella come testo: i numeri con tutte le cifre e il punto.
fn cella(c: &crate::tabella::Cella) -> String {
    use crate::tabella::Cella;
    match c {
        Cella::Vuota => String::new(),
        Cella::Testo(t) => t.clone(),
        Cella::Numero(v) => {
            if v.fract() == 0.0 && v.abs() < 1e15 {
                format!("{}", *v as i64)
            } else {
                format!("{v}")
            }
        }
        // 0,57 · 100 = 56,99999999999999: si arrotonda ai decimali che un numero di Excel porta davvero.
        Cella::Percento(v, _) => format!("{}", (v * 100.0 * 1e10).round() / 1e10),
    }
}

/// Un numero come lo scrive Python («3000.0», «1e-05»): i calcoli degli strumenti confrontano e
/// descrivono i valori con quel testo.
pub fn py_float_str(x: f64) -> String {
    if x.is_nan() {
        return "nan".into();
    }
    if x.is_infinite() {
        return if x > 0.0 { "inf".into() } else { "-inf".into() };
    }
    if x == 0.0 {
        return if x.is_sign_negative() { "-0.0".into() } else { "0.0".into() };
    }
    let e = format!("{x:e}");
    let (mant, exp) = e.split_once('e').unwrap_or((&e, "0"));
    let exp: i32 = exp.parse().unwrap_or(0);
    let neg = mant.starts_with('-');
    let cifre: String = mant.chars().filter(char::is_ascii_digit).collect();
    let segno = if neg { "-" } else { "" };
    if !(-4..16).contains(&exp) {
        let m = if cifre.len() == 1 { cifre.clone() } else { format!("{}.{}", &cifre[..1], &cifre[1..]) };
        return format!("{segno}{m}e{}{:02}", if exp < 0 { '-' } else { '+' }, exp.abs());
    }
    if exp >= 0 {
        let n = exp as usize + 1;
        if cifre.len() <= n {
            format!("{segno}{cifre}{}.0", "0".repeat(n - cifre.len()))
        } else {
            format!("{segno}{}.{}", &cifre[..n], &cifre[n..])
        }
    } else {
        format!("{segno}0.{}{cifre}", "0".repeat((-exp - 1) as usize))
    }
}

/// Un numero scritto da uno strumento: punto o virgola decimale, spazi, segno meno tipografico,
/// percentuale. `None` per testi come «Undetermined», «OVER», «N/A».
pub fn numero(s: &str) -> Option<f64> {
    let t = s.trim().trim_end_matches('%').trim().replace('\u{2212}', "-").replace(['\u{a0}', ' '], "");
    if t.is_empty() {
        return None;
    }
    let t = if t.contains(',') && !t.contains('.') {
        // 1,5 è uno virgola cinque; 1,234,567 sono migliaia.
        if t.matches(',').count() == 1 && !(t.split(',').nth(1).is_some_and(|d| d.len() == 3) && t.split(',').next().is_some_and(|a| a.len() <= 3 && !a.starts_with('0') && !a.starts_with("-0"))) {
            t.replace(',', ".")
        } else if t.matches(',').count() == 1 {
            // «1,234»: con una virgola sola e tre cifre è ambiguo. Gli strumenti scrivono i decimali.
            t.replace(',', ".")
        } else {
            t.replace(',', "")
        }
    } else if t.contains(',') && t.contains('.') {
        if t.rfind(',') > t.rfind('.') {
            t.replace('.', "").replace(',', ".")
        } else {
            t.replace(',', "")
        }
    } else {
        t
    };
    let v: f64 = t.parse().ok()?;
    v.is_finite().then_some(v)
}

/// Le coppie «chiave: valore» (o «chiave<sep>valore») di un blocco di righe, con la chiave pulita.
pub fn coppie(righe: &[Vec<String>]) -> Vec<(String, String)> {
    let mut out = Vec::new();
    for r in righe {
        let celle: Vec<&str> = r.iter().map(|s| s.trim()).filter(|s| !s.is_empty()).collect();
        match celle.as_slice() {
            [una] => {
                if let Some((k, v)) = una.split_once(':') {
                    out.push((k.trim().to_string(), v.trim().to_string()));
                }
            }
            [k, v, ..] => out.push((k.trim_end_matches(':').trim().to_string(), v.to_string())),
            _ => {}
        }
    }
    out
}

/// Il valore di una chiave tra le coppie (senza badare a maiuscole).
pub fn chiave<'a>(coppie: &'a [(String, String)], k: &str) -> Option<&'a str> {
    let k = k.to_ascii_lowercase();
    coppie.iter().find(|(a, _)| a.to_ascii_lowercase() == k).map(|(_, v)| v.as_str()).filter(|v| !v.is_empty())
}

/// L'indice della colonna con uno di questi nomi (senza badare a maiuscole e spazi in più).
pub fn colonna(testa: &[String], nomi: &[&str]) -> Option<usize> {
    let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase();
    let t: Vec<String> = testa.iter().map(|s| norm(s)).collect();
    nomi.iter().find_map(|n| {
        let n = norm(n);
        t.iter().position(|c| *c == n)
    })
}

/// Come [`colonna`], ma basta che il nome della colonna cominci così.
pub fn colonna_inizia(testa: &[String], nomi: &[&str]) -> Option<usize> {
    let norm = |s: &str| s.split_whitespace().collect::<Vec<_>>().join(" ").to_ascii_lowercase();
    let t: Vec<String> = testa.iter().map(|s| norm(s)).collect();
    nomi.iter().find_map(|n| {
        let n = norm(n);
        t.iter().position(|c| c.starts_with(&n))
    })
}

#[cfg(test)]
mod prove {
    use super::*;

    #[test]
    fn i_numeri_degli_strumenti() {
        assert_eq!(numero("0,512"), Some(0.512));
        assert_eq!(numero("1.234,5"), Some(1234.5));
        assert_eq!(numero("1,234.5"), Some(1234.5));
        assert_eq!(numero("−2.5"), Some(-2.5));
        assert_eq!(numero("96.9 %"), Some(96.9));
        assert_eq!(numero("1.2E-05"), Some(1.2e-5));
        for t in ["Undetermined", "OVER", "N/A", "", "-"] {
            assert_eq!(numero(t), None, "{t}");
        }
    }

    #[test]
    fn i_numeri_come_python() {
        for (x, t) in [(3000.0, "3000.0"), (0.5, "0.5"), (-12.25, "-12.25"), (1e16, "1e+16"), (1e-5, "1e-05"), (0.0001, "0.0001"), (123456789.0, "123456789.0"), (1.5e-7, "1.5e-07"), (0.0, "0.0")] {
            assert_eq!(py_float_str(x), t);
        }
    }
}
