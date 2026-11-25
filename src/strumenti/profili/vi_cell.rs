//! Beckman Coulter Vi-CELL BLU (CSV: il riassunto o l'esportazione di dettaglio per immagine) e
//! Vi-CELL XR (Excel o Excel 97 con due righe di intestazione, il testo «chiave : valore» di un
//! campione, o il rapporto di un solo campione).

use super::conta::{custom, micro, nota, Conta};
use super::tabella::Tabella;
use crate::strumenti::fonte::Fonte;
use crate::strumenti::{tempo, Gruppo, Lettura, Misura, Posto, Profilo, Tecnica, Valore};

pub const BLU: Profilo = Profilo {
    id: "beckman_vi_cell_blu",
    nome: "Beckman Coulter Vi-CELL BLU",
    produttore: "Beckman Coulter",
    tecnica: Tecnica::Contacellule,
    estensioni: &["csv"],
    riconosci: riconosci_blu,
    leggi: leggi_blu,
};

pub const XR: Profilo = Profilo {
    id: "beckman_vi_cell_xr",
    nome: "Beckman Coulter Vi-CELL XR",
    produttore: "Beckman Coulter",
    tecnica: Tecnica::Contacellule,
    estensioni: &["xls", "xlsx", "txt"],
    riconosci: riconosci_xr,
    leggi: leggi_xr,
};

/// Un campione letto: i nomi delle colonne e i valori.
type Record = (Vec<String>, Vec<String>);

// ---------------------------------------------------------------- Vi-CELL BLU

fn righe_blu(f: &Fonte) -> Vec<Vec<String>> {
    f.righe_con(',').into_iter().map(|r| r.iter().map(|c| micro(c.trim_start_matches('\u{feff}'))).collect()).collect()
}

/// L'intestazione di una tabella dopo le prime due righe (impostazioni e valori del campione).
fn trova(righe: &[Vec<String>], chiave: &str) -> Option<usize> {
    righe.iter().enumerate().position(|(i, r)| i > 1 && r.first().is_some_and(|c| c == chiave))
}

fn e_dettaglio(righe: &[Vec<String>]) -> bool {
    righe.first().is_some_and(|r| r.first().is_some_and(|c| c == "Sample ID") && !r.iter().any(|c| c == "Viability (%)")) && trova(righe, "Image#").is_some()
}

fn riconosci_blu(f: &Fonte) -> u8 {
    let r = righe_blu(f);
    let Some(testa) = r.first() else { return 0 };
    if testa.first().is_some_and(|c| c == "Sample ID") && ((testa.iter().any(|c| c == "Viability (%)") && testa.iter().any(|c| c.starts_with("Analysis date"))) || e_dettaglio(&r)) {
        92
    } else {
        0
    }
}

/// Un campione dell'esportazione di dettaglio: le impostazioni, il reagente (se uno solo) e la riga «Total».
fn appiattisci(righe: &[Vec<String>]) -> Result<Record, String> {
    let (mut h, mut v) = (righe[0].clone(), righe[1].clone());
    let img = trova(righe, "Image#").ok_or("in un campione non trovo la tabella «Image#»")?;
    if let Some(ri) = trova(&righe[..img], "Part number") {
        let reagenti: Vec<&Vec<String>> = righe[ri + 1..img].iter().filter(|r| r.iter().any(|c| !c.is_empty())).collect();
        if reagenti.len() == 1 {
            h.extend(righe[ri].iter().cloned());
            v.extend(reagenti[0].iter().cloned());
        }
    }
    let totale = righe[img + 1..].iter().find(|r| r.first().is_some_and(|c| c == "Total")).ok_or("in un campione non trovo la riga «Total»")?;
    h.extend(righe[img].iter().skip(1).cloned());
    v.extend(totale.iter().skip(1).cloned());
    Ok((h, v))
}

fn campioni_blu(f: &Fonte) -> Result<Vec<Record>, String> {
    let r = righe_blu(f);
    if r.is_empty() {
        return Err("file vuoto".into());
    }
    if e_dettaglio(&r) {
        let inizi: Vec<usize> = r.iter().enumerate().filter(|(_, x)| **x == r[0]).map(|(i, _)| i).collect();
        return inizi.iter().enumerate().map(|(k, &a)| appiattisci(&r[a..inizi.get(k + 1).copied().unwrap_or(r.len())])).collect();
    }
    let testa = r[0].clone();
    Ok(r[1..].iter().filter(|x| x.iter().any(|c| !c.trim().is_empty())).map(|x| (testa.clone(), x.clone())).collect())
}

fn leggi_blu(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let record = campioni_blu(f)?;
    l.strumento.produttore = "Beckman Coulter".into();
    l.strumento.modello = "Vi-Cell BLU".into();
    l.strumento.software = "Vi-Cell BLU".into();
    l.principale = "viability (cell counter)".into();
    l.unita = "%".into();
    let tabelle: Vec<Tabella> = record.iter().map(|(h, _)| Tabella::nuova(h)).collect();
    let date: Vec<&str> = record.iter().zip(&tabelle).filter_map(|((_, v), t)| t.cella(v, &["analysis date/time"])).collect();
    let gp = tempo::giorno_prima(date.iter().copied());
    let mut gruppi: Vec<(String, Gruppo)> = Vec::new();
    for ((_, v), t) in record.iter().zip(&tabelle) {
        let Some(campione) = t.cella(v, &["sample id"]) else { continue };
        let analista = t.cella(v, &["analysis by"]).unwrap_or("Vi-CELL").to_string();
        let mut m = Misura { id: l.id(), campione: campione.to_string(), quando: t.cella(v, &["analysis date/time"]).and_then(|d| tempo::iso(d, gp)), ..Default::default() };
        let c = Conta {
            tipo_cellula: t.cella(v, &["cell type"]).map(str::to_string),
            diam_min: t.num(v, &["minimum diameter (µm)"]),
            diam_max: t.num(v, &["maximum diameter (µm)"]),
            diluizione: t.num(v, &["dilution"]),
            vitalita: t.num(v, &["viability (%)"]),
            densita_vive: t.num(v, &["viable (x10^6) cells/ml"]),
            totale: t.num(v, &["cell count"]),
            densita_totale: t.num(v, &["total (x10^6) cells/ml"]),
            diam_medio: t.num(v, &["average diameter (µm)"]),
            diam_medio_vive: t.num(v, &["average viable diameter (µm)"]),
            vive: t.num(v, &["viable cells"]),
            circ_media: t.num(v, &["average circularity"]),
            circ_media_vive: t.num(v, &["average viable circularity"]),
            ..Default::default()
        };
        c.applica(&mut m);
        custom(t, v, &["wash", "mixing cycles", "aspiration cycles"], Posto::ImpostazioneAltro, &mut m);
        custom(t, v, &["images", "images for analysis"], Posto::ImmagineAltro, &mut m);
        custom(t, v, &["decluster degree", "minimum circularity", "viable spot area (%)", "viable spot brightness (%)", "cell sharpness", "average background intensity", "bubble count", "cluster count"], Posto::ElaborazioneAltro, &mut m);
        custom(t, v, &["average cells per image"], Posto::ElaboratoAltro, &mut m);
        t.leggi(&["reanalysis date/time", "reanalysis by"]);
        for (h, c) in t.avanzi(v) {
            m.valori.push(nota(&h, &c, Posto::Altro));
        }
        match gruppi.iter_mut().find(|(a, _)| *a == analista) {
            Some((_, g)) => g.misure.push(m),
            None => {
                let mut g = Gruppo { titolo: "Vi-CELL BLU".into(), ..Default::default() };
                g.dati.push(("analyst".into(), analista.clone()));
                g.misure.push(m);
                gruppi.push((analista, g));
            }
        }
    }
    if gruppi.is_empty() {
        return Err("nel file non ci sono campioni".into());
    }
    l.gruppi.extend(gruppi.into_iter().map(|(_, g)| g));
    Ok(())
}

// ---------------------------------------------------------------- Vi-CELL XR

fn riconosci_xr(f: &Fonte) -> u8 {
    let prima = if f.foglio() { f.righe().first().and_then(|r| r.first()).cloned().unwrap_or_default() } else { f.testo.lines().next().unwrap_or("").to_string() };
    if prima.trim().to_lowercase().starts_with("vi-cell xr") {
        92
    } else {
        0
    }
}

/// «Vi-CELL XR 2.06.3» → «2.06».
fn versione(modello: &str) -> String {
    let m = modello.to_lowercase();
    let dopo = m.split_once("xr").map(|(_, d)| d.trim()).unwrap_or("");
    let n: Vec<&str> = dopo.split(|c: char| !(c.is_ascii_digit() || c == '.')).next().unwrap_or("").split('.').collect();
    if n.len() >= 2 && n[0].chars().all(|c| c.is_ascii_digit()) && !n[0].is_empty() && !n[1].is_empty() {
        format!("{}.{}", n[0], n[1])
    } else {
        "2.06".into()
    }
}

/// Le intestazioni di versioni diverse dello stesso dato.
fn rinomina(h: &str) -> String {
    match h {
        "RunDate" | "Sample date/time" | "Sample data/time" => "Sample date".into(),
        "Dilution" => "Dilution factor".into(),
        "Total cells / ml (x 10^6)" => "Total cells/ml (x10^6)".into(),
        "Average diameter (microns)" => "Avg. diam. (microns)".into(),
        "Average circularity" => "Avg. circ.".into(),
        "Total viable cells / ml (x 10^6)" => "Viable cells/ml (x10^6)".into(),
        x => x.into(),
    }
}

/// Il rapporto di un solo campione: i campi stanno in posti fissi.
fn rapporto(r: &[Vec<String>]) -> Record {
    let cella = |i: usize, c: usize| r.get(i).and_then(|x| x.get(c)).map(|s| s.trim().to_string()).unwrap_or_default();
    let mut coppie: Vec<(String, String)> = vec![("Sample ID".into(), cella(3, 2)), ("File name".into(), cella(4, 2)), ("Sample date".into(), cella(5, 2)), ("Comment".into(), cella(6, 2))];
    for (k, i) in [("Total cells", 9), ("Viable cells", 10), ("Viability (%)", 11), ("Total cells/ml (x10^6)", 12), ("Viable cells/ml (x10^6)", 13), ("Avg. diam. (microns)", 14), ("Avg. circ.", 15), ("Images", 16), ("Average cells / image", 17), ("Avg. background intensity", 18)] {
        coppie.push((k.into(), cella(i, 3)));
    }
    for (k, i) in [("Cell type", 9), ("Minimum diameter (microns)", 10), ("Maximum diameter (microns)", 11), ("Minimum circularity", 12), ("Dilution factor", 13), ("Cell brightness (%)", 14), ("Cell sharpness", 15), ("Viable cell spot brightness (%)", 16), ("Viable cell spot area (%)", 17), ("Decluster degree", 18), ("Aspirate cycles", 19), ("Trypan blue mixing cycles", 20)] {
        coppie.push((k.into(), cella(i, 8)));
    }
    coppie.into_iter().unzip()
}

/// L'Excel del Vi-CELL XR: tre righe di dati dello strumento, poi due righe di intestazione.
fn da_foglio(r: &[Vec<String>], versione_file: &str) -> Vec<Record> {
    let testa_riga = if versione_file == "2.04" { 3 } else { 4 };
    let (Some(a), Some(b)) = (r.get(testa_riga), r.get(testa_riga + 1)) else { return Vec::new() };
    let colonne = a.len().max(b.len());
    let testa: Vec<String> = (0..colonne).map(|i| rinomina(format!("{} {}", a.get(i).map(|s| s.trim()).unwrap_or(""), b.get(i).map(|s| s.trim()).unwrap_or("")).replace(" /ml", "/ml").trim())).collect();
    r[testa_riga + 2..].iter().filter(|x| x.iter().any(|c| !c.trim().is_empty())).map(|x| (testa.clone(), x.clone())).collect()
}

/// Il testo di un campione: righe «chiave : valore».
fn da_testo(testo: &str) -> Record {
    testo.lines().filter_map(|riga| riga.split_once(" :")).map(|(k, v)| (rinomina(k.trim()), v.trim().to_string())).unzip()
}

fn leggi_xr(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let (record, modello, seriale): (Vec<Record>, String, Option<String>) = if f.foglio() {
        let r = f.righe();
        let modello = r.first().and_then(|x| x.first()).cloned().unwrap_or_default();
        // Il rapporto di un campione ha «Sample ID» in quarta riga con il valore due celle più in là;
        // nel foglio dei risultati (2.04) la stessa riga è l'intestazione, con «File name» accanto.
        let intestazione = r.get(3).is_some_and(|x| x.get(1).is_some_and(|c| c.trim().eq_ignore_ascii_case("file name")));
        if !intestazione && r.get(3).and_then(|x| x.first()).is_some_and(|c| c.trim() == "Sample ID") {
            (vec![rapporto(&r)], modello, None)
        } else {
            let seriale = r.get(2).and_then(|x| x.first()).and_then(|c| c.split_once(':')).map(|(_, s)| s.trim().to_string()).filter(|s| !s.is_empty());
            let v = versione(&modello);
            (da_foglio(&r, &v), modello, seriale)
        }
    } else {
        let modello = f.testo.lines().next().unwrap_or("").trim().to_string();
        let rec = da_testo(&f.testo);
        let seriale = rec.0.iter().position(|k| k == "Unit S/N").map(|i| rec.1[i].clone()).filter(|s| !s.is_empty());
        (vec![rec], modello, seriale)
    };
    let tabelle: Vec<Tabella> = record.iter().map(|(h, _)| Tabella::nuova(h)).collect();
    // Le righe senza una data leggibile non sono campioni (la seconda riga di intestazione, righe nascoste).
    let date: Vec<Option<String>> = record.iter().zip(&tabelle).map(|((_, v), t)| t.cella(v, &["sample date"]).map(str::to_string)).collect();
    let gp = tempo::giorno_prima(date.iter().flatten().map(String::as_str));
    l.strumento.produttore = "Beckman Coulter".into();
    l.strumento.modello = "Vi-Cell XR".into();
    l.strumento.software = "Vi-Cell XR".into();
    l.strumento.versione = versione(&modello);
    l.strumento.seriale = seriale.unwrap_or_default();
    l.principale = "viability (cell counter)".into();
    l.unita = "%".into();
    let mut g = Gruppo { titolo: "Vi-CELL XR".into(), ..Default::default() };
    g.dati.push(("analyst".into(), "Vi-Cell XR".into()));
    for (((_, v), t), data) in record.iter().zip(&tabelle).zip(&date) {
        let Some(quando) = data.as_deref().and_then(|d| tempo::iso(d, gp)) else { continue };
        let Some(campione) = t.cella(v, &["sample id"]) else { continue };
        let mut m = Misura { id: l.id(), campione: campione.to_string(), quando: Some(quando), ..Default::default() };
        let c = Conta {
            tipo_cellula: t.cella(v, &["cell type"]).map(str::to_string),
            diam_min: t.num(v, &["min diam. (microns)", "minimum diameter (microns)"]),
            diam_max: t.num(v, &["max diam. (microns)", "maximum diameter (microns)"]),
            diluizione: t.num(v, &["dilution factor"]),
            vitalita: t.num(v, &["viability (%)"]),
            densita_vive: t.num(v, &["viable cells/ml (x10^6)"]),
            totale: t.num(v, &["total cells"]),
            densita_totale: t.num(v, &["total cells/ml (x10^6)"]),
            diam_medio: t.num(v, &["avg. diam. (microns)"]),
            vive: t.num(v, &["viable cells"]),
            morte: t.num(v, &["nonviable cells"]),
            circ_media: t.num(v, &["avg. circ."]),
            ..Default::default()
        };
        c.applica(&mut m);
        if let Some(n) = t.cella(v, &["file name"]) {
            m.valori.push(Valore::testo("experimental data identifier", n, Posto::Immagine));
        }
        custom(t, v, &["internal dilution"], Posto::CampioneAltro, &mut m);
        custom(t, v, &["aspirate cycles", "probe volume (ml x 10^-6)", "trypan blue mixing cycles"], Posto::ImpostazioneAltro, &mut m);
        custom(
            t,
            v,
            &["decluster degree", "viable cell spot area (%)", "viable spot area", "viable cell spot brightness (%)", "v. cell spot brightness (%)", "cell sharpness", "sharpness", "cell brightness (%)", "brightness (%)", "brightness", "analysis version", "number of bins", "minimum circularity", "sample depth (microns)"],
            Posto::ElaborazioneAltro,
            &mut m,
        );
        custom(t, v, &["images", "saved images", "imagebasename", "frames", "microns/pixel ratio", "imagedirectory", "image size", "field of view (microns)"], Posto::ImmagineAltro, &mut m);
        custom(t, v, &["avg. cells / image", "average cells / image", "background intensity sum", "total diameter sum", "total circularity sum", "avg. background intensity", "avg. bg intensity"], Posto::ElaboratoAltro, &mut m);
        t.leggi(&["settings:", "results:", "settings", "results"]);
        for (h, c) in t.avanzi(v) {
            m.valori.push(nota(&h, &c, Posto::Altro));
        }
        g.misure.push(m);
    }
    if g.misure.is_empty() {
        return Err("nel file non ci sono campioni con una data leggibile".into());
    }
    l.gruppi.push(g);
    Ok(())
}
