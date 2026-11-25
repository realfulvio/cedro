//! Il file Allotrope (ASM, Allotrope Simple Model): le chiavi dello standard scritte con gli spazi,
//! i dati grezzi dello strumento nelle misure, quelli calcolati in un blocco a parte con le misure da
//! cui vengono. JSON rigoroso: niente NaN; un dato che lo schema vuole e il file non ha si scrive
//! -0 con la nota «Value not provided in instrument file», come fa allotropy.
//!
//! Schemi: plate-reader REC/2025/03, pcr REC/2024/09 (qPCR) e BENCHLING/2023/09 (PCR digitale),
//! spectrophotometry BENCHLING/2023/12, cell-counting REC/2024/09, multi-analyte-profiling
//! BENCHLING/2024/09: gli stessi che usa allotropy.

use serde_json::{json, Map, Value};

use super::{Dato, Lettura, Misura, Posto, Ruolo, Tecnica, Valore};

/// Il nome del convertitore e la sua versione, scritti nel file.
pub const CONVERTITORE: &str = "Cedro";

/// Le scelte di chi converte.
#[derive(Clone, Debug, Default)]
pub struct Opzioni {
    /// I ruoli segnati sulla piastra: pozzetto (A1) → ruolo. Vincono su quelli del file.
    pub ruoli: Vec<(String, Ruolo)>,
    /// Aggiunge i calcoli di Cedro (media dei bianchi e valori meno il bianco), separati dai grezzi.
    pub calcoli: bool,
    /// Il percorso completo del file originale in «UNC path»; altrimenti solo il nome.
    pub percorso: Option<String>,
    /// Il nome del file Allotrope che si scrive.
    pub nome_asm: String,
}

pub fn manifest(t: Tecnica) -> &'static str {
    match t {
        Tecnica::LettorePiastre => "http://purl.allotrope.org/manifests/plate-reader/REC/2025/03/plate-reader.manifest",
        Tecnica::Qpcr => "http://purl.allotrope.org/manifests/pcr/REC/2024/09/qpcr.manifest",
        Tecnica::Dpcr => "http://purl.allotrope.org/manifests/pcr/BENCHLING/2023/09/dpcr.manifest",
        Tecnica::Spettrofotometria => "http://purl.allotrope.org/manifests/spectrophotometry/BENCHLING/2023/12/spectrophotometry.manifest",
        Tecnica::Contacellule => "http://purl.allotrope.org/manifests/cell-counting/REC/2024/09/cell-counting.manifest",
        Tecnica::Multiplex => "http://purl.allotrope.org/manifests/multi-analyte-profiling/BENCHLING/2024/09/multi-analyte-profiling.manifest",
    }
}

/// Lo schema (manifest abbreviato) per le persone: «plate-reader · REC 2025/03».
pub fn schema_breve(t: Tecnica) -> String {
    let m = manifest(t).trim_start_matches("http://purl.allotrope.org/manifests/");
    let p: Vec<&str> = m.split('/').collect();
    format!("{} · {} {}/{}", p.first().copied().unwrap_or(""), p.get(1).copied().unwrap_or(""), p.get(2).copied().unwrap_or(""), p.get(3).copied().unwrap_or(""))
}

fn radice(t: Tecnica) -> (&'static str, &'static str) {
    match t {
        Tecnica::LettorePiastre => ("plate reader aggregate document", "plate reader document"),
        Tecnica::Qpcr => ("qpcr aggregate document", "qpcr document"),
        Tecnica::Dpcr => ("dPCR aggregate document", "dPCR document"),
        Tecnica::Spettrofotometria => ("spectrophotometry aggregate document", "spectrophotometry document"),
        Tecnica::Contacellule => ("cell counting aggregate document", "cell counting document"),
        Tecnica::Multiplex => ("multi analyte profiling aggregate document", "multi analyte profiling document"),
    }
}

fn tipo_strumento(t: Tecnica) -> &'static str {
    match t {
        Tecnica::LettorePiastre => "plate reader",
        Tecnica::Qpcr => "qPCR",
        Tecnica::Dpcr => "dPCR",
        Tecnica::Spettrofotometria => "absorbance detector",
        Tecnica::Contacellule => "brightfield imager (cell counter)",
        Tecnica::Multiplex => "multi analyte profiling analyzer",
    }
}

const NON_DATO: &str = "N/A";
const MANCA: &str = "Value not provided in instrument file";

/// L'unità nello schema: alcuni strumenti scrivono «OD», «RFU», «uL».
fn unita(u: &str) -> String {
    match u.trim() {
        "" => "(unitless)".into(),
        "OD" | "AU" | "Abs" | "A" => "mAU".into(),
        "uL" | "ul" | "µL" => "μL".into(),
        "ng/uL" | "ng/ul" | "ng/μL" | "ng/µl" | "ng/μl" => "ng/µL".into(),
        "ug/uL" | "ug/ul" | "μg/μL" | "µg/µL" | "μg/µL" => "µg/µL".into(),
        "mg/ml" | "mg/mL" => "mg/mL".into(),
        "ug/ml" | "μg/mL" | "µg/mL" | "μg/ml" | "µg/ml" => "µg/mL".into(),
        "ng/ml" | "ng/mL" => "ng/mL".into(),
        "deg C" | "°C" | "C" => "degC".into(),
        "copies/uL" | "copies/µL" | "cp/µL" | "cp/uL" => "#/μL".into(),
        "10^6 cells/ml" => "10^6 cells/mL".into(),
        "um" | "μm" => "µm".into(),
        x => x.to_string(),
    }
}

/// L'assorbanza va in mAU (l'unità che lo schema vuole): lo strumento scrive unità di assorbanza.
fn valore_numero(v: &Valore) -> (f64, String) {
    let u = unita(&v.unita);
    match (v.chiave.as_str(), v.dato.clone()) {
        (k, Dato::Numero(x)) if u == "mAU" && (matches!(v.unita.trim(), "OD" | "AU" | "Abs") || (v.unita.trim() == "A" && k.contains("absorbance"))) => (x * 1000.0, u),
        (_, Dato::Numero(x)) => (x, u),
        _ => (-0.0, u),
    }
}

fn qv(v: f64, u: &str) -> Value {
    json!({"value": num(v), "unit": u})
}

/// Un numero JSON rigoroso: -0 resta -0 (dato mancante), gli interi senza virgola.
fn num(v: f64) -> Value {
    if v == 0.0 && v.is_sign_negative() {
        return serde_json::Number::from_f64(-0.0).map(Value::Number).unwrap_or(json!(0));
    }
    if v.fract() == 0.0 && v.abs() < 9.0e15 {
        return json!(v as i64);
    }
    serde_json::Number::from_f64(v).map(Value::Number).unwrap_or(Value::Null)
}

/// Mette un valore in un oggetto: numero con unità, testo, o -0 con la nota fra gli errori.
fn metti(o: &mut Map<String, Value>, v: &Valore, errori: &mut Vec<(String, String)>) {
    match &v.dato {
        Dato::Numero(_) => {
            let (x, u) = valore_numero(v);
            o.insert(v.chiave.clone(), qv(x, &u));
        }
        Dato::Testo(t) => {
            o.insert(v.chiave.clone(), json!(t));
        }
        Dato::Bool(b) => {
            o.insert(v.chiave.clone(), json!(b));
        }
        Dato::Assente(t) => {
            let (_, u) = valore_numero(v);
            o.insert(v.chiave.clone(), qv(-0.0, &u));
            errori.push((if t.is_empty() { MANCA.to_string() } else { t.clone() }, v.chiave.clone()));
        }
    }
}

fn altro(o: &mut Map<String, Value>, vals: &[&Valore]) {
    if vals.is_empty() {
        return;
    }
    let mut c = Map::new();
    for v in vals {
        let val = match &v.dato {
            Dato::Numero(_) if !v.unita.is_empty() => {
                let (x, u) = valore_numero(v);
                qv(x, &u)
            }
            Dato::Numero(x) => num(*x),
            Dato::Bool(b) => json!(b),
            Dato::Testo(t) | Dato::Assente(t) => json!(t),
        };
        c.insert(v.chiave.clone(), val);
    }
    o.insert("custom information document".into(), Value::Object(c));
}

fn errori_doc(o: &mut Map<String, Value>, errori: &[(String, String)]) {
    if errori.is_empty() {
        return;
    }
    let docs: Vec<Value> = errori.iter().map(|(e, f)| json!({"error": e, "error feature": f})).collect();
    o.insert("error aggregate document".into(), json!({ "error document": docs }));
}

fn ruolo(l: &Lettura, m: &Misura, o: &Opzioni) -> Option<Ruolo> {
    if let Some(p) = &m.pozzetto {
        if let Some((_, r)) = o.ruoli.iter().find(|(w, _)| super::pozzetto_rc(w) == super::pozzetto_rc(p)) {
            return Some(*r);
        }
    }
    let _ = l;
    (m.ruolo_dal_file || m.ruolo != Ruolo::Campione).then_some(m.ruolo)
}

fn misura(l: &Lettura, m: &Misura, o: &Opzioni) -> Value {
    let t = l.tecnica;
    let mut d = Map::new();
    let mut errori: Vec<(String, String)> = m.note.iter().map(|(e, f)| (e.clone(), f.clone())).collect();
    d.insert("measurement identifier".into(), json!(m.id));
    if !matches!(t, Tecnica::LettorePiastre | Tecnica::Spettrofotometria | Tecnica::Multiplex) {
        let quando = m.quando.clone().unwrap_or_else(|| "1970-01-01T00:00:00+00:00".into());
        d.insert("measurement time".into(), json!(quando));
    }
    // Il campione.
    let mut s = Map::new();
    s.insert("sample identifier".into(), json!(if m.campione.is_empty() { m.pozzetto.clone().or_else(|| m.luogo.clone()).unwrap_or_else(|| NON_DATO.to_string()) } else { m.campione.clone() }));
    // Gli strumenti senza piastra (NanoDrop) dicono comunque il posto: A1 del braccio, la posizione.
    let posto = m.pozzetto.clone().or_else(|| if matches!(t, Tecnica::Qpcr | Tecnica::Dpcr) { None } else { m.luogo.clone() });
    if let Some(p) = &posto {
        let p = p.clone();
        match t {
            Tecnica::Dpcr => {
                s.insert("well location identifier".into(), json!(p));
            }
            Tecnica::Qpcr => {
                s.insert("location identifier".into(), json!(m.luogo.clone().unwrap_or_else(|| p.clone())));
                s.insert("well location identifier".into(), json!(p));
            }
            _ => {
                s.insert("location identifier".into(), json!(p));
            }
        }
    }
    if let Some(pl) = &m.piastra {
        s.insert("well plate identifier".into(), json!(pl));
    }
    if let Some(r) = ruolo(l, m, o) {
        s.insert("sample role type".into(), json!(r.asm()));
    }
    let campione: Vec<&Valore> = m.valori.iter().filter(|v| v.posto == Posto::Campione).collect();
    for v in &campione {
        metti(&mut s, v, &mut errori);
    }
    let campione_altro: Vec<&Valore> = m.valori.iter().filter(|v| v.posto == Posto::CampioneAltro).collect();
    altro(&mut s, &campione_altro);
    d.insert("sample document".into(), Value::Object(s));
    // Le impostazioni della lettura.
    let mut dc = Map::new();
    dc.insert("device type".into(), json!(tipo_strumento(t)));
    if t == Tecnica::Qpcr && !m.valori.iter().any(|v| v.chiave == "measurement method identifier") {
        dc.insert("measurement method identifier".into(), json!(NON_DATO));
    }
    for v in m.valori.iter().filter(|v| v.posto == Posto::Impostazione) {
        metti(&mut dc, v, &mut errori);
    }
    let impost_altro: Vec<&Valore> = m.valori.iter().filter(|v| v.posto == Posto::ImpostazioneAltro).collect();
    altro(&mut dc, &impost_altro);
    d.insert("device control aggregate document".into(), json!({ "device control document": [Value::Object(dc)] }));
    if matches!(t, Tecnica::Qpcr | Tecnica::Dpcr) {
        d.insert("target DNA description".into(), json!(m.bersaglio.clone().unwrap_or_else(|| NON_DATO.into())));
    }
    for v in m.valori.iter().filter(|v| v.posto == Posto::Misura) {
        metti(&mut d, v, &mut errori);
    }
    // I dati elaborati dallo strumento.
    let elab: Vec<&Valore> = m.valori.iter().filter(|v| v.posto == Posto::Elaborato).collect();
    let impost: Vec<&Valore> = m.valori.iter().filter(|v| v.posto == Posto::Elaborazione).collect();
    let elab_altro: Vec<&Valore> = m.valori.iter().filter(|v| v.posto == Posto::ElaboratoAltro).collect();
    let impost_altro: Vec<&Valore> = m.valori.iter().filter(|v| v.posto == Posto::ElaborazioneAltro).collect();
    if !elab.is_empty() || !impost.is_empty() || !elab_altro.is_empty() || !impost_altro.is_empty() {
        let mut p = Map::new();
        if !impost.is_empty() || !impost_altro.is_empty() {
            let mut dp = Map::new();
            for v in &impost {
                metti(&mut dp, v, &mut errori);
            }
            altro(&mut dp, &impost_altro);
            p.insert("data processing document".into(), Value::Object(dp));
        }
        for v in &elab {
            metti(&mut p, v, &mut errori);
        }
        altro(&mut p, &elab_altro);
        d.insert("processed data aggregate document".into(), json!({ "processed data document": [Value::Object(p)] }));
    }
    // Le immagini (contacellule): il file dei dati e le note.
    let immagine: Vec<&Valore> = m.valori.iter().filter(|v| v.posto == Posto::Immagine).collect();
    let immagine_altro: Vec<&Valore> = m.valori.iter().filter(|v| v.posto == Posto::ImmagineAltro).collect();
    if !immagine.is_empty() || !immagine_altro.is_empty() {
        let mut im = Map::new();
        for v in &immagine {
            metti(&mut im, v, &mut errori);
        }
        altro(&mut im, &immagine_altro);
        d.insert("image aggregate document".into(), json!({ "image document": [Value::Object(im)] }));
    }
    if !m.spettro.is_empty() {
        // La curva: lunghezze d'onda (o secondi) e valori; l'assorbanza va in mAU come le altre assorbanze del file.
        let (chiave, etichetta, asse, concetto, unita_y, fattore) = match &m.curva {
            Some(c) => {
                let (u, f) = if matches!(c.unita.trim(), "OD" | "AU" | "Abs") { ("mAU".to_string(), 1000.0) } else { (unita(&c.unita), 1.0) };
                let asse = if c.cinetica { ("elapsed time", "s") } else { ("wavelength", "nm") };
                (c.chiave.clone(), c.etichetta.clone(), asse, c.concetto.clone(), u, f)
            }
            None => ("absorption spectrum data cube".to_string(), "absorption spectrum".to_string(), ("wavelength", "nm"), "absorbance".to_string(), "mAU".to_string(), 1000.0),
        };
        let onde: Vec<Value> = m.spettro.iter().map(|(w, _)| num(*w)).collect();
        // Un punto che lo strumento non ha dato (NaN) è -0, come ogni dato mancante.
        let ass: Vec<Value> = m.spettro.iter().map(|(_, a)| if a.is_finite() { num(a * fattore) } else { num(-0.0) }).collect();
        d.insert(
            chiave,
            json!({
                "label": etichetta,
                "cube-structure": {
                    "dimensions": [{"@componentDatatype": "double", "concept": asse.0, "unit": asse.1}],
                    "measures": [{"@componentDatatype": "double", "concept": concetto, "unit": unita_y}],
                },
                "data": {"dimensions": [onde], "measures": [ass]},
            }),
        );
    }
    // Il testo al posto del valore principale («Undetermined») in Allotrope è già l'errore del dato mancante.
    let extra: Vec<&Valore> = m.valori.iter().filter(|v| v.posto == Posto::Altro && !(v.chiave == l.principale && matches!(v.dato, Dato::Testo(_)))).collect();
    altro(&mut d, &extra);
    errori_doc(&mut d, &errori);
    Value::Object(d)
}

/// Il file Allotrope della lettura.
pub fn scrivi(l: &Lettura, o: &Opzioni) -> Value {
    let t = l.tecnica;
    let (agg, docname) = radice(t);
    let mut root = Map::new();
    let mut ds = Map::new();
    ds.insert("ASM file identifier".into(), json!(if o.nome_asm.is_empty() { format!("{}.json", stem(&l.file)) } else { o.nome_asm.clone() }));
    ds.insert("data system instance identifier".into(), json!(if l.strumento.istanza.is_empty() { NON_DATO } else { &l.strumento.istanza }));
    ds.insert("ASM converter name".into(), json!(CONVERTITORE));
    ds.insert("ASM converter version".into(), json!(env!("CARGO_PKG_VERSION")));
    ds.insert("file name".into(), json!(l.file));
    ds.insert("UNC path".into(), json!(o.percorso.clone().unwrap_or_else(|| l.file.clone())));
    ds.insert("software name".into(), json!(if l.strumento.software.is_empty() { NON_DATO } else { &l.strumento.software }));
    if !l.strumento.versione.is_empty() {
        ds.insert("software version".into(), json!(l.strumento.versione));
    }
    altro(&mut ds, &l.sistema.iter().collect::<Vec<_>>());
    root.insert("data system document".into(), Value::Object(ds));
    let mut dv = Map::new();
    let s = &l.strumento;
    dv.insert("device identifier".into(), json!(if s.identificativo.is_empty() { if s.modello.is_empty() { NON_DATO } else { &s.modello } } else { &s.identificativo }));
    dv.insert("model number".into(), json!(if s.modello.is_empty() { NON_DATO } else { &s.modello }));
    dv.insert("equipment serial number".into(), json!(if s.seriale.is_empty() { NON_DATO } else { &s.seriale }));
    if !s.produttore.is_empty() {
        dv.insert("product manufacturer".into(), json!(s.produttore));
    }
    for (k, v) in [("asset management identifier", &s.inventario), ("description", &s.descrizione), ("brand name", &s.marchio)] {
        if !v.is_empty() {
            dv.insert(k.into(), json!(v));
        }
    }
    altro(&mut dv, &l.dispositivo.iter().collect::<Vec<_>>());
    root.insert("device system document".into(), Value::Object(dv));

    let mut docs = Vec::new();
    // Dove lo schema vuole l'ora della misura: nel documento di insieme (una per ora diversa) o in ogni misura.
    let ora_di_insieme = matches!(t, Tecnica::LettorePiastre | Tecnica::Spettrofotometria | Tecnica::Multiplex);
    for (gi, g) in l.gruppi.iter().enumerate() {
        let mut parti: Vec<(Option<String>, Vec<&Misura>)> = Vec::new();
        if ora_di_insieme {
            for m in &g.misure {
                let q = m.quando.clone().or_else(|| g.quando.clone());
                match parti.iter_mut().find(|(k, _)| *k == q) {
                    Some((_, v)) => v.push(m),
                    None => parti.push((q, vec![m])),
                }
            }
        } else {
            parti.push((None, g.misure.iter().collect()));
        }
        for (quando, misure) in parti {
            let mut ma = Map::new();
            let docs_misure: Vec<Value> = misure.iter().map(|m| misura(l, m, o)).collect();
            ma.insert("measurement document".into(), Value::Array(docs_misure));
            let mut errori: Vec<(String, String)> = Vec::new();
            if ora_di_insieme {
                ma.insert("measurement time".into(), json!(quando.unwrap_or_else(|| "1970-01-01T00:00:00+00:00".into())));
            }
            if matches!(t, Tecnica::LettorePiastre | Tecnica::Qpcr | Tecnica::Dpcr | Tecnica::Multiplex) && misure.iter().any(|m| m.pozzetto.is_some()) {
                ma.insert("container type".into(), json!("well plate"));
                if let Some(n) = g.pozzetti {
                    ma.insert("plate well count".into(), qv(n as f64, "#"));
                } else if matches!(t, Tecnica::LettorePiastre | Tecnica::Qpcr) {
                    let (r, c) = l.formato(gi);
                    ma.insert("plate well count".into(), qv((r * c) as f64, "#"));
                }
            }
            if t == Tecnica::Qpcr {
                ma.insert("experimental data identifier".into(), json!(dato(g, "experimental data identifier").unwrap_or(NON_DATO)));
                ma.insert("experiment type".into(), json!(dato(g, "experiment type").unwrap_or("comparative CT qPCR experiment")));
                if !g.dati.iter().any(|(k, _)| k == "well volume") {
                    ma.insert("well volume".into(), qv(-0.0, "μL"));
                    errori.push((MANCA.into(), "well volume".into()));
                }
            }
            // I contacellule tengono le note del gruppo nel loro documento di informazioni.
            if t == Tecnica::Contacellule {
                let note: Vec<Valore> = g.dati.iter().filter(|(k, _)| k != "analyst").map(|(k, v)| Valore::testo(k, v, Posto::Altro)).collect();
                altro(&mut ma, &note.iter().collect::<Vec<_>>());
            }
            // Le voci del gruppo che iniziano con «~» sono informazioni libere sulla corsa.
            let libere: Vec<Valore> = g.dati.iter().filter_map(|(k, v)| k.strip_prefix('~').map(|k| match super::fonte::numero(v) {
                Some(x) => Valore::num(k, x, "", Posto::Altro),
                _ => Valore::testo(k, v, Posto::Altro),
            })).collect();
            altro(&mut ma, &libere.iter().collect::<Vec<_>>());
            for (k, v) in &g.dati {
                if t == Tecnica::Contacellule || k == "analyst" || k.starts_with('~') || (t == Tecnica::Qpcr && matches!(k.as_str(), "experimental data identifier" | "experiment type")) {
                    continue;
                }
                match super::fonte::numero(v) {
                    Some(x) if k == "well volume" => {
                        ma.insert(k.clone(), qv(x, "μL"));
                    }
                    _ => {
                        ma.insert(k.clone(), json!(v));
                    }
                }
            }
            errori_doc(&mut ma, &errori);
            let mut doc = Map::new();
            if let Some(a) = dato(g, "analyst") {
                doc.insert("analyst".into(), json!(a));
            }
            doc.insert("measurement aggregate document".into(), Value::Object(ma));
            docs.push(Value::Object(doc));
        }
    }
    root.insert(docname.into(), Value::Array(docs));

    // I calcoli: quelli dello strumento e, se chiesti, quelli di Cedro (bianco).
    let mut calc: Vec<Value> = Vec::new();
    let mut n = 0usize;
    let id_calcolo = |i: usize| format!("{}_CALC_{i}", stem(&l.file).to_uppercase().replace(|c: char| !c.is_ascii_alphanumeric(), "_"));
    let mut push = |nome: &str, v: f64, u: &str, sorgenti: &[String], da: &str, descrizione: &str, altri: &[usize]| {
        let id = id_calcolo(n);
        n += 1;
        let mut src: Vec<Value> = sorgenti.iter().map(|s| json!({"data source identifier": s, "data source feature": da})).collect();
        // Un calcolo che viene da altri calcoli: la fonte è il calcolo e la «feature» il suo nome.
        for &a in altri {
            if let Some(c) = l.calcolati.get(a) {
                src.push(json!({"data source identifier": id_calcolo(a), "data source feature": c.nome}));
            }
        }
        let chiave = if t == Tecnica::Dpcr { "calculated datum" } else { "calculated result" };
        let mut c = json!({
            "calculated data identifier": id,
            "calculated data name": nome,
            chiave: qv(v, &unita(u)),
            "data source aggregate document": {"data source document": src},
        });
        if !descrizione.is_empty() {
            c["calculation description"] = json!(descrizione);
        }
        calc.push(c);
    };
    for c in &l.calcolati {
        push(&c.nome, c.valore, &c.unita, &c.sorgenti, &c.da, &c.descrizione, &c.da_calcoli);
    }
    if o.calcoli {
        for (nome, v, u, src) in calcoli_cedro(l, o) {
            push(&nome, v, &u, &src, &l.principale, "", &[]);
        }
    }
    if !calc.is_empty() {
        root.insert("calculated data aggregate document".into(), json!({ "calculated data document": calc }));
    }
    let mut top = Map::new();
    top.insert("$asm.manifest".into(), json!(manifest(t)));
    top.insert(agg.into(), Value::Object(root));
    Value::Object(top)
}

fn dato<'a>(g: &'a super::Gruppo, k: &str) -> Option<&'a str> {
    g.dati.iter().find(|(a, _)| a == k).map(|(_, v)| v.as_str())
}

fn stem(f: &str) -> String {
    std::path::Path::new(f).file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_else(|| f.to_string())
}

/// I calcoli di Cedro sui pozzetti segnati come bianco: la media dei bianchi e ogni valore meno quella.
pub fn calcoli_cedro(l: &Lettura, o: &Opzioni) -> Vec<(String, f64, String, Vec<String>)> {
    let mut out = Vec::new();
    for g in &l.gruppi {
        let bianchi: Vec<&Misura> = g.misure.iter().filter(|m| ruolo(l, m, o) == Some(Ruolo::Bianco) && l.valore(m).is_some()).collect();
        if bianchi.is_empty() {
            continue;
        }
        let media = bianchi.iter().filter_map(|m| l.valore(m)).sum::<f64>() / bianchi.len() as f64;
        let u = unita_calcolo(l);
        out.push((format!("{} media dei bianchi (Cedro)", g.titolo).trim().to_string(), media * scala(l), u.clone(), bianchi.iter().map(|m| m.id.clone()).collect()));
        for m in &g.misure {
            if let Some(v) = l.valore(m) {
                out.push((format!("{} meno il bianco (Cedro)", m.pozzetto.clone().unwrap_or_else(|| m.campione.clone())), (v - media) * scala(l), u.clone(), vec![m.id.clone()]));
            }
        }
    }
    out
}

fn scala(l: &Lettura) -> f64 {
    if l.principale.contains("absorbance") && matches!(l.unita.trim(), "OD" | "AU" | "Abs" | "A" | "") {
        1000.0
    } else {
        1.0
    }
}

fn unita_calcolo(l: &Lettura) -> String {
    if scala(l) == 1000.0 {
        "mAU".into()
    } else {
        unita(&l.unita)
    }
}

/// Il CSV piatto: una riga per misura, con lo strumento e le impostazioni ripetuti. Per chi non legge il JSON.
pub fn piatto(l: &Lettura, o: &Opzioni) -> String {
    let mut chiavi: Vec<String> = Vec::new();
    for m in l.misure() {
        for v in &m.valori {
            if !matches!(v.posto, Posto::Altro) && !chiavi.contains(&v.chiave) {
                chiavi.push(v.chiave.clone());
            }
        }
    }
    let mut testa: Vec<String> = ["file", "strumento", "produttore", "software", "gruppo", "misura", "quando", "pozzetto", "piastra", "campione", "ruolo", "bersaglio"].iter().map(|s| s.to_string()).collect();
    testa.extend(chiavi.iter().map(|k| {
        let u = l.misure().find_map(|m| m.valore(k)).map(|v| v.unita.clone()).unwrap_or_default();
        if u.is_empty() {
            k.clone()
        } else {
            format!("{k} ({u})")
        }
    }));
    let mut righe = vec![testa];
    for g in &l.gruppi {
        for m in g.misure.iter().filter(|m| !m.e_spettro()) {
            let mut r = vec![
                l.file.clone(),
                l.nome.to_string(),
                l.strumento.produttore.clone(),
                l.strumento.software.clone(),
                g.titolo.clone(),
                m.id.clone(),
                m.quando.clone().or_else(|| g.quando.clone()).unwrap_or_default(),
                m.pozzetto.clone().unwrap_or_default(),
                m.piastra.clone().or_else(|| g.piastra.clone()).unwrap_or_default(),
                m.campione.clone(),
                ruolo(l, m, o).unwrap_or(Ruolo::Campione).nome().to_string(),
                m.bersaglio.clone().unwrap_or_default(),
            ];
            for k in &chiavi {
                r.push(match m.valore(k).map(|v| &v.dato) {
                    Some(Dato::Numero(x)) => format!("{x}"),
                    Some(Dato::Bool(b)) => if *b { "true".into() } else { "false".into() },
                    Some(Dato::Testo(t)) | Some(Dato::Assente(t)) => t.clone(),
                    None => String::new(),
                });
            }
            righe.push(r);
        }
    }
    crate::textconv::write_csv(&righe, ',')
}

/// Il controllo leggero del file scritto: le chiavi che lo schema vuole sempre ci sono, le misure
/// hanno identificatori unici e nessun numero è NaN. Lo schema completo si applica nelle prove.
pub fn controlla(v: &Value) -> Result<(), String> {
    let manifest = v.get("$asm.manifest").and_then(Value::as_str).ok_or("manca il manifest")?;
    if !manifest.starts_with("http://purl.allotrope.org/manifests/") {
        return Err("manifest non valido".into());
    }
    let root = v.as_object().and_then(|o| o.iter().find(|(k, _)| k.ends_with("aggregate document"))).map(|(_, v)| v).ok_or("manca il documento principale")?;
    for k in ["data system document", "device system document"] {
        if root.get(k).is_none() {
            return Err(format!("manca «{k}»"));
        }
    }
    let mut ids = std::collections::HashSet::new();
    let mut errore = None;
    fn visita(v: &Value, ids: &mut std::collections::HashSet<String>, errore: &mut Option<String>) {
        match v {
            Value::Object(o) => {
                for (k, x) in o {
                    if k == "measurement identifier" || k == "calculated data identifier" {
                        if let Some(s) = x.as_str() {
                            if !ids.insert(s.to_string()) {
                                *errore = Some(format!("identificatore ripetuto: {s}"));
                            }
                        }
                    }
                    if let Some(u) = x.get("unit") {
                        if u.as_str().is_none_or(str::is_empty) || x.get("value").is_none_or(|n| !n.is_number()) {
                            *errore = Some(format!("valore senza numero o unità: {k}"));
                        }
                    }
                    visita(x, ids, errore);
                }
            }
            Value::Array(a) => a.iter().for_each(|x| visita(x, ids, errore)),
            _ => {}
        }
    }
    visita(root, &mut ids, &mut errore);
    match errore {
        Some(e) => Err(e),
        None => Ok(()),
    }
}
