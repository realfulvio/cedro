//! Unchained Labs Lunatic e Stunner: l'export dei risultati (csv o Excel) di un lettore di piastre per
//! assorbanza e per diffusione dinamica della luce. Ogni riga è un pozzetto. In testa può esserci un blocco
//! di informazioni (data, operatore, strumento, applicazione) o niente: allora le informazioni sono nelle
//! colonne di ogni riga.
//!
//! Una colonna di assorbanza («A260», «A280», «A260 (10mm)»…) dà il valore; più colonne sono uno spettro.
//! Le colonne calcolate dal programma (concentrazione, rapporti A260/A280, diametri, polidispersione…) sono
//! i calcoli del pozzetto, con la misura da cui vengono.

use crate::strumenti::fonte::{numero, Fonte};
use crate::strumenti::{tempo, Calcolato, Curva, Dato, Gruppo, Lettura, Misura, Posto, Profilo, Ruolo, Tecnica, Valore, PRINCIPALE_CURVA};

use super::serie::{con_cifre, Serie};

pub const PROFILO: Profilo = Profilo {
    id: "unchained_labs_lunatic_stunner",
    nome: "Unchained Labs Lunatic & Stunner",
    produttore: "Unchained Labs",
    tecnica: Tecnica::LettorePiastre,
    estensioni: &["csv", "xlsx"],
    riconosci,
    leggi,
};

// ---------------------------------------------------------------- la tabella

/// Il file diviso in intestazione (nome e valore) e tabella dei pozzetti.
struct Libro {
    h_nomi: Vec<String>,
    h_celle: Vec<Option<String>>,
    nomi: Vec<String>,
    righe: Vec<Vec<Option<String>>>,
}

fn righe_del_file(f: &Fonte) -> Vec<Vec<String>> {
    if let Some(p) = f.fogli.first() {
        p.righe.clone()
    } else {
        f.righe()
    }
}

fn pulito(c: &str) -> String {
    c.replace('\n', " ").replace('\r', "").trim().to_lowercase()
}

fn riga_testa(righe: &[Vec<String>]) -> Option<usize> {
    righe.iter().position(|r| r.iter().any(|c| c.to_lowercase().contains("sample name")))
}

/// Il nome di una colonna di assorbanza: «a260» o «a260 (10mm)». Ritorna la lunghezza d'onda.
fn onda_di(k: &str) -> Option<u32> {
    let r = k.strip_prefix('a')?;
    if r.len() < 3 || !r.is_char_boundary(3) {
        return None;
    }
    let (cifre, resto) = r.split_at(3);
    if !cifre.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    let ok = resto.is_empty() || {
        let t = resto.strip_prefix(' ').and_then(|t| t.strip_prefix('('));
        t.and_then(|t| t.strip_suffix("mm)")).is_some_and(|n| n.chars().all(|c| c.is_ascii_digit()))
    };
    ok.then(|| cifre.parse().ok()).flatten()
}

fn carica(f: &Fonte) -> Result<Libro, String> {
    let righe = righe_del_file(f);
    if righe.iter().all(|r| r.iter().all(|c| c.trim().is_empty())) {
        return Err("il file è vuoto".into());
    }
    let idx = riga_testa(&righe).ok_or("non trovo la riga con «Sample name» che apre la tabella")?;
    // Il blocco di informazioni sopra la tabella: vuoti e «N/A» sono mancanti; si tolgono le righe e le
    // colonne tutte mancanti; resta una colonna di nomi e una di valori.
    let blocco: Vec<Vec<Option<String>>> = righe[..idx]
        .iter()
        .map(|r| r.iter().map(|c| if c.trim().is_empty() || c.trim().eq_ignore_ascii_case("n/a") || c.trim().eq_ignore_ascii_case("na") { None } else { Some(c.clone()) }).collect::<Vec<_>>())
        .filter(|r: &Vec<Option<String>>| r.iter().any(Option::is_some))
        .collect();
    let larghezza = blocco.iter().map(Vec::len).max().unwrap_or(0);
    let colonne: Vec<Vec<Option<String>>> = (0..larghezza).map(|c| blocco.iter().map(|r| r.get(c).cloned().flatten()).collect::<Vec<_>>()).filter(|c| c.iter().any(Option::is_some)).collect();

    let nomi: Vec<String> = righe[idx].iter().map(|c| pulito(c)).collect();
    let sample = nomi.iter().position(|n| n.contains("sample name"));
    let mut dati: Vec<Vec<Option<String>>> = Vec::new();
    for r in &righe[idx + 1..] {
        if r.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        // Una riga senza il nome del campione (la riga finisce prima) è una misura saltata.
        if sample.is_some_and(|i| r.len() <= i) {
            continue;
        }
        dati.push(r.iter().cloned().map(Some).collect());
    }
    if dati.is_empty() {
        return Err("nel file non ci sono righe di risultati".into());
    }

    let senza_testata = idx == 0 || (colonne.len() == 1 && blocco.len() == 1);
    let (h_nomi, h_celle) = if senza_testata {
        (nomi.clone(), dati[0].clone())
    } else {
        let valori = colonne.get(1).ok_or("nell'intestazione del file ci sono i nomi ma non i valori")?;
        let mut n = Vec::new();
        let mut c = Vec::new();
        for (l, v) in colonne[0].iter().zip(valori.iter()) {
            if v.is_some() {
                n.push(l.as_deref().map(|s| s.trim().to_lowercase()).unwrap_or_else(|| "nan".into()));
                c.push(v.clone());
            }
        }
        (n, c)
    };
    Ok(Libro { h_nomi, h_celle, nomi, righe: dati })
}

// ---------------------------------------------------------------- riconoscimento

fn riconosci(f: &Fonte) -> u8 {
    if f.foglio() {
        if f.fogli.first().map_or(true, |p| !p.righe.iter().take(60).any(|r| r.iter().any(|c| c.to_lowercase().contains("sample name")))) {
            return 0;
        }
    } else if !f.testo.chars().take(8192).collect::<String>().to_lowercase().contains("sample name") {
        return 0;
    }
    let righe = righe_del_file(f);
    let Some(idx) = riga_testa(&righe) else { return 0 };
    let nomi: Vec<String> = righe[idx].iter().map(|c| pulito(c)).collect();
    let con_piastra = nomi.iter().any(|n| n == "plate position" || n == "plate id");
    let con_onde = nomi.iter().any(|n| onda_di(n).is_some());
    if con_piastra && con_onde {
        92
    } else {
        0
    }
}

// ---------------------------------------------------------------- i calcoli del programma

/// Una colonna calcolata: nome, colonna canonica (e alternative), unità e da che cosa viene.
struct Voce {
    nome: &'static str,
    colonna: &'static str,
    alt: &'static [&'static str],
    unita: &'static str,
    da: &'static str,
    /// La colonna con la formula, per i calcoli di adattamento.
    formula: Option<&'static str>,
}

const ASS: &str = "absorbance";
const DLS: &str = "dynamic light scattering";

const fn v(nome: &'static str, colonna: &'static str, alt: &'static [&'static str], unita: &'static str, da: &'static str) -> Voce {
    Voce { nome, colonna, alt, unita, da, formula: None }
}

const fn vf(nome: &'static str, colonna: &'static str, unita: &'static str, da: &'static str, formula: &'static str) -> Voce {
    Voce { nome, colonna, alt: &[], unita, da, formula: Some(formula) }
}

/// I calcoli cercati con una colonna A260.
const CALCOLI_A260: &[Voce] = &[
    v("Concentration", "a260 concentration (ng/ul)", &[], "ng/µL", ASS),
    v("Concentration", "concentration (ng/ul)", &[], "ng/µL", ASS),
    v("Background (A260)", "background (a260)", &[], "AU", ASS),
    v("A260/A230", "a260/a230", &[], "", ASS),
    v("A260/A280", "a260/a280", &[], "", ASS),
];

/// I calcoli cercati con una colonna A280, compresi quelli della diffusione dinamica della luce.
const CALCOLI_A280: &[Voce] = &[
    v("Concentration", "concentration (mg/ml)", &[], "mg/mL", ASS),
    v("Background (A280)", "background (a280)", &[], "AU", ASS),
    v("A260/A280", "a260/a280", &[], "", ASS),
    v("KC/R (mol/g)", "kc/r (mol/g)", &[], "", DLS),
    v("kD (ml/g)", "kd (ml/g)", &[], "", DLS),
    v("B22", "b22 (ml.mol/g^2)", &[], "", DLS),
    v("Optical contrast constant K", "optical contrast constant k (m^2 mol kg^-2)", &[], "", DLS),
    v("Z Average Diameter", "z ave. dia (nm)", &["z-avg dia. (nm)"], "nm", DLS),
    v("Polydispersity Index", "pdi", &[], "", DLS),
    v("Diameter Standard Deviation", "sd dia (nm)", &["sd dia. (nm)"], "nm", DLS),
    v("Diffusion coefficient", "diffusion coefficient (um^2/s)", &[], "", DLS),
    v("Peak of Interest Mean Diameter", "peak of interest mean dia (nm)", &["pkoi intensity mean dia. (nm)"], "nm", DLS),
    v("Peak of Interest Mode Diameter", "peak of interest mode dia (nm)", &[], "nm", DLS),
    v("Peak of Interest Est. MW", "peak of interest est. mw (kda)", &["pkoi est. mw (kda)"], "kDa", DLS),
    v("Peak of Interest Intensity", "peak of interest intensity (%)", &["pkoi intensity area (%)"], "%", DLS),
    v("Peak of Interest Mass", "peak of interest mass (%)", &["pkoi mass area (%)"], "%", DLS),
    v("Peak of Interest Diffusion coefficient", "peak of interest diffusion coefficient (um^2/s)", &[], "", DLS),
    v("Peak of Interest Mass Mean Diameter", "peak of interest mass mean dia (nm)", &["pkoi mass mean dia. (nm)"], "nm", DLS),
    v("Peak of Interest Rayleigh Ratio R", "peak of interest rayleigh ratio r (cm^-1)", &["pkoi rayleigh ratio r (1/km)"], "", DLS),
    v("Derived intensity (cps)", "derived intensity (cps)", &[], "", DLS),
    v("Rayleigh ratio R", "rayleigh ratio r (cm^-1)", &["rayleigh ratio r (1/km)"], "", DLS),
    vf("kD goodness of fit", "kd goodness of fit", "", DLS, "kd linear fit"),
    vf("B22 goodness of fit", "b22 goodness of fit", "", DLS, "b22 linear fit"),
    v("Viscosity at T (cP)", "viscosity at t (cp)", &[], "", DLS),
    v("Viscosity at 20°C (cP)", "viscosity at 20°c (cp)", &["viscosity at 25°c (cp)"], "", DLS),
    v("RI at T", "ri at t", &[], "", DLS),
    v("RI at 20°C", "ri at 20°c", &["ri at 25°c"], "", DLS),
    v("Diameter @ C=0", "diameter @ c=0 (nm)", &[], "nm", DLS),
    v("Number of Peaks", "number of peaks", &[], "", DLS),
    v("Number of Angles", "number of angles", &[], "", DLS),
    v("Angles Measured", "angles measured (°)", &[], "", DLS),
    v("Intercept", "intercept", &[], "", DLS),
];

fn voci_per(col: &str) -> &'static [Voce] {
    match col {
        "a260" => CALCOLI_A260,
        "a280" => CALCOLI_A280,
        _ => &[],
    }
}

/// Un calcolo letto da una riga: la voce, il valore (NaN = «N/A» nel file) e la formula se c'è.
struct Letto {
    voce: &'static Voce,
    valore: f64,
    formula: Option<String>,
}

fn calcoli_di_riga(d: &Serie, onde: &[(usize, String, u32)], errori: &mut Vec<(String, String)>) -> Vec<Letto> {
    let mut out: Vec<Letto> = Vec::new();
    for (_, col, _) in onde {
        for voce in voci_per(col) {
            let mut valore: Option<f64> = None;
            let mut e_na = false;
            for c in std::iter::once(&voce.colonna).chain(voce.alt.iter()) {
                let grezzo = d.grezzo(c).map(str::trim).unwrap_or("");
                valore = None;
                e_na = false;
                if grezzo.is_empty() {
                    continue;
                }
                if grezzo.eq_ignore_ascii_case("n/a") {
                    valore = Some(f64::NAN);
                    e_na = true;
                } else {
                    valore = numero(grezzo);
                }
                if valore.is_some() {
                    break;
                }
            }
            if e_na {
                errori.push(("N/A".into(), voce.nome.into()));
            }
            if let Some(x) = valore {
                let formula = voce.formula.and_then(|k| d.grezzo(k)).map(str::to_string).filter(|t| !t.trim().is_empty());
                match out.iter_mut().find(|o| o.voce.colonna == voce.colonna) {
                    Some(o) => {
                        o.valore = x;
                        o.formula = formula;
                    }
                    None => out.push(Letto { voce, valore: x, formula }),
                }
            }
        }
    }
    out
}

// ---------------------------------------------------------------- i picchi della diffusione della luce

const CAMPI_PICCO: &[(&str, &str, &[&str])] = &[
    ("peak mean diameter", "nm", &["mean dia (nm)", "intensity mean dia. (nm)"]),
    ("peak mode diameter", "nm", &["mode dia (nm)"]),
    ("peak est. MW", "kDa", &["est. mw (kda)"]),
    ("peak intensity", "%", &["intensity (%)", "intensity area (%)"]),
    ("peak mass", "%", &["mass (%)", "mass area (%)"]),
    ("peak mass mean diameter", "", &["mass mean dia. (nm)"]),
    ("peak rayleigh ratio", "", &["rayleigh ratio r (1/km)"]),
    ("peak diffusion coefficient", "", &["diffusion coefficient (um^2/s)"]),
];

fn picchi_di(d: &Serie, nomi: &[String]) -> Vec<Vec<Valore>> {
    let mut numeri: Vec<u32> = Vec::new();
    let mut nuovo_formato = false;
    for n in nomi {
        // «peak 1 …» (vecchio) e «pk1 …» (nuovo): il numero e poi uno spazio.
        for (pre, nuovo) in [("peak ", false), ("pk", true)] {
            if con_cifre(n, pre) {
                let cifre: String = n[pre.len()..].chars().take_while(char::is_ascii_digit).collect();
                if n[pre.len() + cifre.len()..].starts_with(' ') {
                    if let Ok(k) = cifre.parse() {
                        if !numeri.contains(&k) {
                            numeri.push(k);
                        }
                        nuovo_formato |= nuovo;
                    }
                    break;
                }
            }
        }
    }
    numeri.sort_unstable();
    let mut picchi = Vec::new();
    for k in numeri {
        let prefisso = if nuovo_formato { format!("pk{k}") } else { format!("peak {k}") };
        let mut p: Vec<Valore> = Vec::new();
        for (nome, unita, colonne) in CAMPI_PICCO {
            let cols: Vec<String> = colonne.iter().map(|c| format!("{prefisso} {c}")).collect();
            let refs: Vec<&str> = cols.iter().map(String::as_str).collect();
            if let Some(x) = d.num(&refs) {
                p.push(Valore::num(nome, x, unita, Posto::Altro));
            }
        }
        if !p.is_empty() {
            p.push(Valore::testo("peak index", &k.to_string(), Posto::Altro));
            picchi.push(p);
        }
    }
    picchi
}

// ---------------------------------------------------------------- lo spettro

fn decimali(t: &str) -> usize {
    let t = t.trim().split(['e', 'E']).next().unwrap_or("");
    t.split_once('.').map_or(0, |(_, d)| d.len())
}

fn arrotonda(x: f64, d: usize) -> f64 {
    let f = 10f64.powi(d as i32);
    (x * f).round() / f
}

/// Lunghezze d'onda in ordine e assorbanze; due colonne con la stessa lunghezza d'onda (A260 e A260 (10mm))
/// si uniscono tenendo il valore con più decimali, se non si contraddicono.
fn spettro_di(d: &Serie, onde: &[(usize, String, u32)]) -> Result<Vec<(f64, f64)>, String> {
    let mut v: Vec<(u32, Option<(f64, String)>)> = Vec::new();
    for (_, col, w) in onde {
        let grezzo = d.grezzo(col).map(str::trim).filter(|t| !t.is_empty()).and_then(|t| numero(t).map(|x| (x, t.to_string())));
        match v.iter_mut().find(|(k, _)| k == w) {
            None => v.push((*w, grezzo)),
            Some((_, prima)) => {
                if let (Some(a), Some(b)) = (prima.clone(), grezzo) {
                    let basso = decimali(&a.1).min(decimali(&b.1));
                    if arrotonda(a.0, basso) != arrotonda(b.0, basso) {
                        return Err(format!("due colonne danno valori diversi per la stessa lunghezza d'onda ({w} nm): {} e {}", a.1, b.1));
                    }
                    if decimali(&b.1) > decimali(&a.1) {
                        *prima = Some(b);
                    }
                }
            }
        }
    }
    v.sort_by_key(|(w, _)| *w);
    Ok(v.into_iter().map(|(w, a)| (w as f64, a.map_or(f64::NAN, |x| x.0))).collect())
}

// ---------------------------------------------------------------- la lettura

fn salta_misura(k: &str) -> bool {
    const SALTA: [&str; 11] = ["time", "row", "concentration factor (ng/ul)", "application", "concentration (ng/ul)", "background wvl. (nm)", "plate id", "plate position", "plate type", "instrument id", "column"];
    SALTA.contains(&k) || e_a3(k) || e_a3_10mm(k) || e_a3_su_a3(k)
}

fn tre_cifre(s: &str) -> bool {
    s.len() == 3 && s.chars().all(|c| c.is_ascii_digit())
}

/// «a260»
fn e_a3(k: &str) -> bool {
    k.strip_prefix('a').is_some_and(tre_cifre)
}
/// «a260 (10mm)»
fn e_a3_10mm(k: &str) -> bool {
    k.strip_prefix('a').and_then(|r| r.strip_suffix(" (10mm)")).is_some_and(tre_cifre)
}
/// «a260/a280»
fn e_a3_su_a3(k: &str) -> bool {
    k.strip_prefix('a').and_then(|r| r.split_once("/a")).is_some_and(|(a, b)| tre_cifre(a) && tre_cifre(b))
}

fn salta_testata(k: &str) -> bool {
    const ESATTI: [&str; 29] = [
        "time", "row", "path length mode", "sample group", "pump", "concentration factor (ng/ul)", "sample name", "application", "concentration (ng/ul)", "e1%", "concentration (mg/ml)", "background wvl. (nm)", "plate id", "plate position", "analyte", "buffer", "b/s/f/r", "acquisition filtering", "number of acquisitions",
        "number of acquisitions used", "number of acquisitions (total)", "number of acquisitions used (total)", "acquisition time (s)", "temperature (°c)", "molecular weight (kda)", "intercept", "number of peaks", "number of angles", "pdi",
    ];
    if ESATTI.contains(&k) {
        return true;
    }
    const INIZI: [&str; 15] = ["angles measured", "sd dia", "diffusion coefficient", "rayleigh ratio r", "optical contrast constant", "viscosity at", "ri at", "derived intensity", "diameter @", "kc/r", "kd ", "b22", "pkoi", "peak of interest", "z"];
    // «z.avg dia…»: una «z», un carattere qualsiasi e «avg dia».
    let z_avg = k.starts_with('z') && k.chars().nth(1).is_some_and(|_| k.chars().skip(2).collect::<String>().starts_with("avg dia"));
    if INIZI.iter().take(14).any(|p| k.starts_with(p)) || z_avg {
        return true;
    }
    // «pk1 …» e «peak 1 …».
    if con_cifre(k, "pk") || con_cifre(k, "peak ") {
        return true;
    }
    if k.strip_prefix("background (a").and_then(|r| r.strip_suffix(')')).is_some_and(tre_cifre) {
        return true;
    }
    if k.strip_prefix('a').and_then(|r| r.strip_suffix(" concentration (ng/ul)")).is_some_and(tre_cifre) {
        return true;
    }
    e_a3(k) || e_a3_10mm(k) || e_a3_su_a3(k)
}

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let libro = carica(f)?;
    let onde: Vec<(usize, String, u32)> = libro.nomi.iter().enumerate().filter_map(|(i, n)| onda_di(n).map(|w| (i, n.clone(), w))).collect();
    if onde.is_empty() {
        return Err("nel file ci deve essere una colonna di assorbanza (per esempio A280)".into());
    }
    let testata = Serie::nuova(&libro.h_nomi, &libro.h_celle);

    // L'ordine delle date: lo dicono le date con il giorno sopra 12.
    let mut date: Vec<String> = Vec::new();
    if let Some(d) = testata.testo(&["date"]) {
        date.push(d);
    }
    for r in &libro.righe {
        if let Some(i) = libro.nomi.iter().position(|n| n == "date") {
            if let Some(Some(d)) = r.get(i) {
                date.push(d.clone());
            }
        }
    }
    let giorno_prima = tempo::giorno_prima(date.iter().map(String::as_str));

    let spettro = onde.len() > 1;
    let applicazione = testata.testo(&["application"]);
    let dls = applicazione.as_deref().is_some_and(|a| a.contains("B22 & kD"));
    let id_strumento = testata.testo_pieno(&["instrument id"]).or_else(|| testata.testo_pieno(&["instrument"])).ok_or("non trovo l'identificativo dello strumento: ci vuole la colonna «Instrument ID» o «Instrument» nell'intestazione")?;
    let analista = testata.testo(&["test performed by"]);

    let mut letti: Vec<Vec<Letto>> = Vec::new();
    let mut tutte: Vec<Misura> = Vec::new();
    for r in &libro.righe {
        let d = Serie::nuova(&libro.nomi, r);
        // Quando.
        let mut quando = testata.testo(&["date"]);
        let giorno = d.testo(&["date"]);
        let ora = d.testo(&["time"]);
        if let (Some(g), Some(o)) = (giorno.as_deref().filter(|g| !g.trim().is_empty()), ora.as_deref().filter(|o| !o.trim().is_empty())) {
            let g = g.trim().strip_suffix(" 00:00:00").unwrap_or(g.trim());
            quando = Some(format!("{g} {}", o.trim()));
        }
        let quando = quando.filter(|q| !q.trim().is_empty()).ok_or("non trovo la data e l'ora: ci vogliono le colonne «Date» e «Time» oppure la «Date» nell'intestazione")?;
        let quando_iso = tempo::iso(&quando, giorno_prima).ok_or_else(|| format!("non capisco la data «{quando}»"))?;

        let posizione = d.grezzo("plate position").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).ok_or("manca la posizione del pozzetto (colonna «Plate position»)")?;
        let campione = d.grezzo("sample name").map(str::to_string).unwrap_or_default();
        let piastra = d.testo_pieno(&["plate id"]);

        let mut m = Misura { id: l.id(), pozzetto: Some(posizione), piastra: piastra.clone(), campione, quando: Some(quando_iso), ..Default::default() };
        if let Some(b) = d.testo_pieno(&["b/s/f/r"]) {
            let ruolo = match b.trim().to_uppercase().as_str() {
                "B" => Some(Ruolo::Bianco),
                "S" => Some(Ruolo::Campione),
                "R" => Some(Ruolo::Riferimento),
                _ => None,
            };
            if let Some(r) = ruolo {
                m.ruolo = r;
                m.ruolo_dal_file = true;
            }
            m.valori.push(Valore::testo("sample role type", b.trim(), Posto::CampioneAltro));
        }

        // Le impostazioni.
        m.valori.push(Valore::testo("detection type", if dls { "Dynamic Light Scattering" } else { "Absorbance" }, Posto::Impostazione));
        if let Some(v) = testata.testo(&["client version"]) {
            m.valori.push(Valore::testo("firmware version", &v, Posto::Impostazione));
        }
        let sfondo = d.num(&["background wvl. (nm)"]);
        let sfondo_ass = sfondo.and_then(|w| d.num(&[&format!("background (a{})", w as i64)]));
        if let Some(x) = d.num(&["number of acquisitions", "number of acquisitions (total)"]) {
            m.valori.push(Valore::num("number of averages", x, "#", Posto::Impostazione));
        }
        if let Some(x) = d.num(&["acquisition time (s)"]) {
            m.valori.push(Valore::num("integration time", x, "s", Posto::Impostazione));
        }
        if let Some(w) = sfondo {
            m.valori.push(Valore::num("electronic absorbance reference wavelength setting", w, "nm", Posto::Impostazione));
        }
        // Le impostazioni libere del dispositivo.
        for (chiave, ks) in [("path length mode", vec!["path length mode"]), ("pump", vec!["pump"])] {
            if let Some(t) = d.testo_pieno(&ks) {
                m.valori.push(Valore::testo(chiave, &t, Posto::ImpostazioneAltro));
            }
        }
        if let Some(t) = testata.testo_pieno(&["column"]).or_else(|| d.testo_pieno(&["column"])) {
            m.valori.push(Valore::testo("column", &t, Posto::ImpostazioneAltro));
        }
        if let Some(t) = d.testo_pieno(&["number of acquisitions used", "number of acquisitions used (total)"]) {
            m.valori.push(Valore::testo("Number of acquisitions used", &t, Posto::ImpostazioneAltro));
        }
        if let Some(t) = d.testo_pieno(&["acquisition filtering"]) {
            m.valori.push(Valore::testo("Acquisition filtering", &t, Posto::ImpostazioneAltro));
        }

        // I valori.
        let mut errori: Vec<(String, String)> = Vec::new();
        if spettro {
            m.spettro = spettro_di(&d, &onde)?;
            for (_, col, _) in &onde {
                if d.grezzo(col).is_some_and(|t| t.trim().eq_ignore_ascii_case("n/a")) {
                    errori.push(("N/A".into(), "absorbance".into()));
                }
            }
            m.curva = Some(Curva { chiave: "absorption spectrum data cube".into(), etichetta: "absorbance-spectrum".into(), cinetica: false, concetto: "absorbance".into(), unita: "AU".into() });
        } else {
            let (_, col, w) = &onde[0];
            let grezzo = d.grezzo(col).map(str::trim).unwrap_or("");
            m.valori.push(match numero(grezzo) {
                Some(x) => Valore::num("absorbance", if x == 0.0 { 0.0 } else { x }, "AU", Posto::Misura),
                None => Valore { chiave: "absorbance".into(), dato: Dato::Assente(if grezzo.is_empty() { "N/A".into() } else { grezzo.to_string() }), unita: "AU".into(), posto: Posto::Misura },
            });
            m.valori.push(Valore::num("detector wavelength setting", *w as f64, "nm", Posto::Impostazione));
        }
        if let Some(t) = d.num(&["temperature (°c)"]) {
            m.valori.push(Valore::num("compartment temperature", t, "°C", Posto::Misura));
        }
        let metodo = d.testo_pieno(&["application"]).or_else(|| applicazione.clone().filter(|a| !a.trim().is_empty()));
        if let Some(t) = metodo {
            m.valori.push(Valore::testo("analytical method identifier", &t, Posto::Misura));
        }
        if let Some(t) = testata.testo(&["experiment name"]).filter(|t| !t.trim().is_empty()) {
            m.valori.push(Valore::testo("experimental data identifier", &t, Posto::Misura));
        }

        // Il campione.
        if let Some(t) = d.testo_pieno(&["sample group"]) {
            m.valori.push(Valore::testo("batch identifier", &t, Posto::Campione));
        }
        let tipo_piastra = testata.testo_pieno(&["plate type"]).or_else(|| d.testo_pieno(&["plate type"]));
        for (chiave, v) in [
            ("plate type", tipo_piastra),
            ("nr of plates", testata.testo_pieno(&["nr of plates"])),
            ("blanks", testata.testo_pieno(&["blanks"])),
            ("plate description", testata.testo_pieno(&["nan"])),
        ] {
            if let Some(t) = v {
                m.valori.push(Valore::testo(chiave, &t, Posto::CampioneAltro));
            }
        }
        if let Some(x) = d.num(&["e1%"]) {
            m.valori.push(Valore::num("molar attenuation coefficient setting", x, "", Posto::CampioneAltro));
        }
        for chiave in ["analyte", "buffer"] {
            if let Some(t) = d.testo_pieno(&[chiave]) {
                m.valori.push(Valore::testo(chiave, &t, Posto::CampioneAltro));
            }
        }
        if let Some(x) = d.num(&["molecular weight (kda)"]) {
            m.valori.push(Valore::num("molecular weight (kda)", x, "", Posto::CampioneAltro));
        }

        // Il fattore di concentrazione e i picchi.
        if let Some(x) = d.num(&["concentration factor (ng/ul)"]) {
            m.valori.push(Valore::num("concentration factor", x, "ng/µL", Posto::Elaborazione));
        }
        m.picchi = picchi_di(&d, &libro.nomi);

        // I calcoli del programma.
        let calcoli = calcoli_di_riga(&d, &onde, &mut errori);
        for k in ["b22 linear fit", "kd linear fit"] {
            d.segna(k);
        }
        m.note = errori;
        // Le altre colonne, come note della misura.
        if let Some(x) = sfondo_ass {
            m.valori.push(Valore::num("electronic absorbance reference absorbance", x, "AU", Posto::Altro));
        }
        for (k, dato) in d.non_letti(&salta_misura) {
            m.valori.push(Valore { chiave: k, dato, unita: String::new(), posto: Posto::Altro });
        }

        letti.push(calcoli);
        tutte.push(m);
    }

    // Le misure per piastra, nell'ordine del file.
    let mut piastre: Vec<(String, Gruppo)> = Vec::new();
    for m in &tutte {
        let nome = m.piastra.clone().unwrap_or_default();
        if !piastre.iter().any(|(p, _)| *p == nome) {
            let mut g = Gruppo { titolo: if nome.is_empty() { "Piastra 1".into() } else { nome.clone() }, quando: m.quando.clone(), piastra: m.piastra.clone(), pozzetti: Some(96), ..Default::default() };
            if let Some(a) = &analista {
                g.dati.push(("analyst".into(), a.clone()));
            }
            piastre.push((nome.clone(), g));
        }
        if let Some((_, g)) = piastre.iter_mut().find(|(p, _)| *p == nome) {
            g.misure.push(m.clone());
        }
    }

    // I calcoli, nell'ordine delle colonne del programma: per ogni calcolo, tutte le misure che lo hanno.
    let mut gia: Vec<(&str, &str)> = Vec::new();
    let mut ordine: Vec<&'static Voce> = Vec::new();
    for voce in CALCOLI_A260.iter().chain(CALCOLI_A280.iter()) {
        if !gia.contains(&(voce.nome, voce.colonna)) {
            gia.push((voce.nome, voce.colonna));
            ordine.push(voce);
        }
    }
    for voce in ordine {
        for (i, calcoli) in letti.iter().enumerate() {
            if let Some(c) = calcoli.iter().find(|c| c.voce.colonna == voce.colonna) {
                l.calcolati.push(Calcolato { nome: c.voce.nome.into(), valore: c.valore, unita: c.voce.unita.into(), sorgenti: vec![tutte[i].id.clone()], da: c.voce.da.into(), descrizione: c.formula.clone().unwrap_or_default(), da_calcoli: Vec::new() });
            }
        }
    }

    l.strumento.modello = "Lunatic".into();
    l.strumento.identificativo = id_strumento;
    l.strumento.software = "Lunatic and Stunner Analysis".into();
    l.strumento.versione = testata.testo(&["software version"]).unwrap_or_default();
    l.principale = if spettro { PRINCIPALE_CURVA.into() } else { "absorbance".into() };
    l.unita = "AU".into();
    if let Some(a) = &applicazione {
        l.info.push(("Applicazione".into(), a.clone()));
    }
    if let Some(e) = testata.testo(&["experiment name"]).filter(|t| !t.trim().is_empty()) {
        l.info.push(("Esperimento".into(), e));
    }
    if let Some(a) = &analista {
        l.info.push(("Operatore".into(), a.clone()));
    }
    for (_, g) in piastre {
        l.gruppi.push(g);
    }
    // Le informazioni dell'intestazione che nessuna misura ha usato.
    for (k, dato) in testata.non_letti(&salta_testata) {
        l.sistema.push(Valore { chiave: k, dato, unita: String::new(), posto: Posto::Altro });
    }
    Ok(())
}
