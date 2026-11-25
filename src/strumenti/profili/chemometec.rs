//! ChemoMetec NC-View (CSV con una riga per campione) e NucleoView (l'export di un campione: elenco
//! «chiave;valore», oppure «chiave:⇥valore»).

use super::conta::{in_gruppo, micro, nota, numero_con_nota, Conta};
use super::tabella::Tabella;
use crate::strumenti::fonte::Fonte;
use crate::strumenti::{tempo, Calcolato, Lettura, Misura, Posto, Profilo, Tecnica, Valore};

pub const NC_VIEW: Profilo = Profilo {
    id: "chemometec_nc_view",
    nome: "ChemoMetec NC-View",
    produttore: "ChemoMetec",
    tecnica: Tecnica::Contacellule,
    estensioni: &["csv"],
    riconosci: riconosci_nc_view,
    leggi: leggi_nc_view,
};

pub const NUCLEOVIEW: Profilo = Profilo {
    id: "chemometec_nucleoview",
    nome: "ChemoMetec NucleoView",
    produttore: "ChemoMetec",
    tecnica: Tecnica::Contacellule,
    estensioni: &["csv", "txt"],
    riconosci: riconosci_nucleoview,
    leggi: leggi_nucleoview,
};

// ---------------------------------------------------------------- NC-View

fn righe_nc(f: &Fonte) -> Vec<Vec<String>> {
    f.righe_con(',').into_iter().map(|r| r.iter().map(|c| micro(c.trim_start_matches('\u{feff}'))).collect()).collect()
}

fn riconosci_nc_view(f: &Fonte) -> u8 {
    if f.foglio() {
        return 0;
    }
    let Some(testa) = righe_nc(f).into_iter().next() else { return 0 };
    let ha = |n: &str| testa.iter().any(|c| c.trim().eq_ignore_ascii_case(n));
    if ha("INSTRUMENT") && ha("VIABILITY (%)") && ha("SAMPLE ID") {
        92
    } else {
        0
    }
}

/// «20240219-00019» → il giorno dell'analisi (il nome del file non dice l'ora).
fn giorno_del_nome(nome: &str) -> Option<String> {
    let g = nome.split('-').next()?.trim();
    if g.len() == 8 && g.bytes().all(|b| b.is_ascii_digit()) {
        Some(format!("{}-{}-{}T00:00:00+00:00", &g[..4], &g[4..6], &g[6..8]))
    } else {
        None
    }
}

fn leggi_nc_view(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let righe = righe_nc(f);
    let testa = righe.first().ok_or("file vuoto")?;
    let t = Tabella::nuova(testa);
    let dati: Vec<&Vec<String>> = righe[1..].iter().filter(|r| r.iter().any(|c| !c.trim().is_empty())).collect();
    let primo = dati.first().ok_or("nel file non ci sono campioni")?;
    l.strumento.produttore = "ChemoMetec".into();
    l.strumento.software = "NC-View".into();
    // «S/N: 9002020102201902».
    l.strumento.seriale = t.cella(primo, &["instrument"]).map(|s| s.rsplit(':').next().unwrap_or(s).trim().to_string()).unwrap_or_default();
    if let Some(u) = t.cella(primo, &["user"]) {
        l.sistema.push(Valore::testo("USER", u, Posto::Altro));
    }
    l.principale = "viability (cell counter)".into();
    l.unita = "%".into();
    let milioni = |v: Option<f64>| v.map(super::conta::milioni);
    let mut gruppi = Vec::new();
    for r in dati {
        let Some(campione) = t.cella(r, &["sample id"]) else { continue };
        let mut m = Misura { id: l.id(), campione: campione.to_string(), quando: t.cella(r, &["name"]).and_then(giorno_del_nome), ..Default::default() };
        let c = Conta {
            rivelatore: "fluorescence",
            dispositivo: Some("Cell counter"),
            vitalita: t.num(r, &["viability (%)"]),
            densita_vive: milioni(t.num(r, &["live (cells/ml)"])),
            densita_morte: milioni(t.num(r, &["dead (cells/ml)"])),
            densita_totale: milioni(t.num(r, &["total (cells/ml)"])),
            diam_medio: t.num(r, &["diameter (µm)"]),
            diluizione: t.num(r, &["dilution factor"]),
            ..Default::default()
        };
        c.applica(&mut m);
        if let Some(v) = t.num(r, &["aggregates (%)"]) {
            m.valori.push(Valore::num("cell aggregation percentage", v, "%", Posto::ElaboratoAltro));
        }
        if let Some(v) = t.num(r, &["debris index"]) {
            m.valori.push(Valore::num("debris index", v, "(unitless)", Posto::ElaboratoAltro));
        }
        if let Some(v) = t.cella(r, &["media"]) {
            m.valori.push(Valore::testo("MEDIA", v, Posto::CampioneAltro));
        }
        if let Some(v) = t.cella(r, &["protocol"]) {
            m.valori.push(Valore::testo("method name", v, Posto::Altro));
        }
        if let Some(v) = t.cella(r, &["status"]) {
            m.valori.push(Valore::testo("STATUS", v, Posto::Altro));
        }
        t.leggi(&["instrument", "user"]);
        for (h, c) in t.avanzi(r) {
            m.valori.push(nota(&h, &c, Posto::Altro));
        }
        let operatore = t.cella(r, &["operator"]).unwrap_or("NC-View").to_string();
        in_gruppo(&mut gruppi, "NC-View", vec![("analyst".into(), operatore)], m);
    }
    if gruppi.is_empty() {
        return Err("nel file non ci sono campioni".into());
    }
    l.gruppi.extend(gruppi);
    Ok(())
}

// ---------------------------------------------------------------- NucleoView

/// Le coppie chiave-valore del file. Con «:⇥» fra chiave e valore il file è dell'export nuovo; altrimenti
/// sono righe «chiave;valore;». Una riga che continua la precedente (una parentesi rimasta aperta) la
/// si attacca a quella.
fn voci(testo: &str) -> Vec<(String, String)> {
    let a_tab = testo.lines().any(|r| r.contains(":\t"));
    let mut out: Vec<(String, String)> = Vec::new();
    for riga in testo.lines() {
        let (k, v) = if a_tab {
            match riga.split_once(":\t") {
                Some((k, v)) => (k.trim(), v.trim()),
                None => (riga.trim(), ""),
            }
        } else {
            let mut p = riga.split(';');
            (p.next().unwrap_or("").trim(), p.next().unwrap_or("").trim().trim_end_matches(',').trim())
        };
        if k.is_empty() {
            continue;
        }
        if v.is_empty() {
            if let Some(ultima) = out.last_mut() {
                if ultima.1.matches('(').count() > ultima.1.matches(')').count() {
                    ultima.1.push(' ');
                    ultima.1.push_str(k.trim_end_matches(')').trim_end());
                    ultima.1.push(')');
                }
            }
            continue;
        }
        out.push((k.to_string(), v.to_string()));
    }
    out
}

fn riconosci_nucleoview(f: &Fonte) -> u8 {
    if f.foglio() {
        return 0;
    }
    let v = voci(&f.testo);
    let ha = |n: &str| v.iter().any(|(k, _)| k.eq_ignore_ascii_case(n));
    let densita = ["live (cells/ml)", "dead (cells/ml)", "total (cells/ml)"].iter().filter(|n| ha(n)).count();
    if ha("viability (%)") && densita >= 2 && (ha("image") || ha("instrument type") || ha("sample id")) {
        90
    } else {
        0
    }
}

/// «20231120 163016» con lo scarto orario in ore («2», «-7»): l'ora con il suo fuso.
fn ora_nucleoview(data: &str, scarto: Option<&str>) -> Option<String> {
    let mut p = data.split_whitespace();
    let (d, h) = (p.next()?, p.next()?);
    if d.len() != 8 || h.len() != 6 || !d.bytes().chain(h.bytes()).all(|b| b.is_ascii_digit()) {
        return None;
    }
    let ore: f64 = scarto.map_or(Some(0.0), |s| s.trim().trim_start_matches('+').parse().ok())?;
    let minuti = (ore.abs() * 60.0).round() as u32;
    Some(format!("{}-{}-{}T{}:{}:{}{}{:02}:{:02}", &d[..4], &d[4..6], &d[6..8], &h[..2], &h[2..4], &h[4..6], if ore < 0.0 { '-' } else { '+' }, minuti / 60, minuti % 60))
}

/// «s/n 9000201099909999» e «S/N:99999999999» → il numero.
fn seriale(s: &str) -> String {
    let t = s.trim();
    match t.get(..3) {
        Some(a) if a.eq_ignore_ascii_case("s/n") => t[3..].trim_start_matches([':', ' ']).trim().to_string(),
        _ => t.to_string(),
    }
}

fn leggi_nucleoview(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let v = voci(&f.testo);
    let chiavi: Vec<String> = v.iter().map(|(k, _)| k.clone()).collect();
    let r: Vec<String> = v.iter().map(|(_, x)| x.clone()).collect();
    let t = Tabella::nuova(&chiavi);
    let vitalita = t.num(&r, &["viability (%)"]).ok_or("nel file non c'è la vitalità")?;
    l.strumento.produttore = "ChemoMetec".into();
    l.strumento.modello = t.cella(&r, &["instrument type"]).unwrap_or("NucleoCounter NC-200").to_string();
    l.strumento.software = "NucleoView".into();
    l.strumento.versione = t.prima(&r, &["application sw version", "software version"]).unwrap_or_default().to_string();
    l.strumento.seriale = t.prima(&r, &["instrument s/n", "instrument"]).map(seriale).unwrap_or_default();
    l.strumento.istanza = t.cella(&r, &["pc"]).unwrap_or_default().to_string();
    if let Some(x) = t.cella(&r, &["csv file version"]) {
        l.sistema.push(nota("csv file version", x, Posto::Altro));
    }
    if let Some(x) = t.cella(&r, &["21 CFR Part 11"]) {
        l.sistema.push(Valore::testo("21 CFR Part 11", x, Posto::Altro));
    }
    l.principale = "viability (cell counter)".into();
    l.unita = "%".into();

    let quando = t.cella(&r, &["date time"]).and_then(|d| ora_nucleoview(d, t.cella(&r, &["time zone offset"]))).or_else(|| t.cella(&r, &["time (utc)"]).and_then(|d| tempo::iso(d, false)));
    t.leggi(&["time zone offset"]);
    let immagine = t.cella(&r, &["image"]);
    let campione = t.cella(&r, &["sample id"]).map(str::to_string).or_else(|| immagine.and_then(|i| i.split('-').nth(3)).map(str::to_string)).unwrap_or_else(|| f.stem().to_string());
    let mut m = Misura { id: l.id(), campione, quando, ..Default::default() };

    // Le densità arrivano in cellule/mL; i valori con una nota («3.58E3 (Out of optimal range)») restano numeri, con la nota.
    let mut note: Vec<Valore> = Vec::new();
    let mut con_nota = |nomi: &[&str]| -> Option<f64> {
        let s = t.cella(&r, nomi)?;
        let (x, n) = numero_con_nota(s);
        if let Some(n) = n {
            let chiave = nomi.iter().find_map(|a| t.indice(a)).map(|i| t.testa[i].clone()).unwrap_or_default();
            note.push(Valore::testo(&chiave, &n, Posto::ElaboratoAltro));
        }
        x
    };
    let vive = con_nota(&["live (cells/ml)"]).map(super::conta::milioni);
    let morte = con_nota(&["dead (cells/ml)"]).map(super::conta::milioni);
    let totale = con_nota(&["total (cells/ml)"]).map(super::conta::milioni);
    let diametro = con_nota(&["estimated cell diameter (um)", "estimated cell diameter [um]", "diameter (um)"]);
    let c = Conta {
        rivelatore: "dark field",
        dispositivo: Some("dark field imager (cell counter)"),
        volume: t.num(&r, &["sample volume (ul)"]),
        diluizione: t.num(&r, &["multiplication factor", "dilution factor"]),
        vitalita: Some(vitalita),
        densita_vive: vive,
        densita_morte: morte,
        densita_totale: totale,
        diam_medio: diametro,
        ..Default::default()
    };
    c.applica(&mut m);
    m.valori.extend(note);
    if let Some(x) = t.num(&r, &["dilution volume (ul)"]) {
        m.valori.push(Valore::num("dilution volume", x, "μL", Posto::CampioneAltro));
    }
    if let Some(x) = t.cella(&r, &["sample media"]) {
        m.valori.push(Valore::testo("Sample Media", x, Posto::CampioneAltro));
    }
    if let Some(x) = t.cella(&r, &["cm filename"]) {
        m.valori.push(Valore::testo("experimental data identifier", x, Posto::Immagine));
    }
    if let Some(x) = t.num(&r, &["aggregates (%)"]) {
        m.valori.push(Valore::num("cell aggregation percentage", x, "%", Posto::ElaboratoAltro));
    }
    if let Some(x) = t.num(&r, &["debris index"]) {
        m.valori.push(Valore::num("debris index", x, "(unitless)", Posto::ElaboratoAltro));
    }
    for (nome, chiave, unita, da) in [("cell diameter standard deviation (um)", "Cell diameter standard deviation (um)", "µm", "Average Total Cell Diameter"), ("(%) of cells in aggregates with five or more cells", "Percentage of cells with five or more", "%", "Cell count")] {
        if let Some(x) = t.num(&r, &[nome]).filter(|x| *x != 0.0) {
            l.calcolati.push(Calcolato { nome: chiave.into(), valore: x, unita: unita.into(), sorgenti: vec![m.id.clone()], da: da.into(), descrizione: String::new(), da_calcoli: Vec::new() });
        } else {
            t.leggi(&[nome]);
        }
    }
    // Le informazioni del protocollo e dell'utente stanno nel gruppo; il resto, nelle note della misura.
    let operatore = t.cella(&r, &["operator"]).unwrap_or("NC-200").to_string();
    let mut dati = vec![("analyst".to_string(), operatore)];
    for k in ["Protocol template purpose", "Protocol template filename", "Protocol template title", "Protocol adaptation purpose", "Protocol adaptation title", "Protocol adaptation filename", "Login ID", "Event log format"] {
        if let Some(x) = t.cella(&r, &[k]) {
            dati.push((k.to_string(), x.to_string()));
        }
    }
    for (h, c) in t.avanzi(&r) {
        m.valori.push(nota(&h, &c, Posto::Altro));
    }
    let mut gruppi = Vec::new();
    in_gruppo(&mut gruppi, "NucleoView", dati, m);
    l.gruppi.extend(gruppi);
    Ok(())
}
