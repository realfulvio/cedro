//! Bio-Rad Bio-Plex Manager: l'XML che il programma esporta dopo una lettura multiplex a sfere
//! (Bio-Plex 100/200, 3D). Contiene i pozzetti letti (corsa, condizioni, sfere contate per regione con le
//! statistiche della fluorescenza) e, a parte, i campioni con i loro pozzetti e i nomi degli analiti.

use std::collections::HashMap;

use roxmltree::{Document, Node};

use crate::strumenti::fonte::{numero, Fonte};
use crate::strumenti::{tempo, Analita, Gruppo, Lettura, Misura, Posto, Profilo, Ruolo, Statistica, Tecnica, Valore};

pub const PROFILO: Profilo = Profilo {
    id: "biorad_bioplex_manager",
    nome: "Bio-Rad Bio-Plex Manager",
    produttore: "Bio-Rad",
    tecnica: Tecnica::Multiplex,
    estensioni: &["xml"],
    riconosci,
    leggi,
};

/// Le statistiche per regione: il tag del file, il ruolo in Allotrope e l'unità.
const STATISTICHE: &[(&str, &str, &str)] = &[
    ("Median", "median", "RFU"),
    ("Mean", "arithmetic mean", "RFU"),
    ("CV", "coefficient of variation", ""),
    ("StdDev", "standard deviation", ""),
    ("TrimmedMean", "trimmed arithmetic mean", "RFU"),
    ("TrimmedStdDev", "trimmed standard deviation", ""),
];

/// I codici di errore della lettura di un analita.
const ERRORI: &[(&str, &str)] = &[("1", "Low bead number"), ("2", "Aggregated beads"), ("3", "Classify efficiency"), ("4", "Region selection"), ("5", "Platform temperature")];

fn riconosci(f: &Fonte) -> u8 {
    let t = &f.testo;
    if t.contains("<Samples") && t.contains("<Wells") && t.contains("<PlateDimensions") {
        if t.contains("BioPlexManager") {
            95
        } else {
            70
        }
    } else {
        0
    }
}

// ---------------------------------------------------------------- l'XML

fn figli<'a, 'b>(n: Node<'a, 'b>, nome: &'static str) -> impl Iterator<Item = Node<'a, 'b>> {
    n.children().filter(move |c| c.is_element() && c.tag_name().name() == nome)
}

fn figlio<'a, 'b>(n: Node<'a, 'b>, nome: &'static str) -> Option<Node<'a, 'b>> {
    figli(n, nome).next()
}

fn testo(n: Node) -> String {
    n.text().unwrap_or("").trim().to_string()
}

/// Il testo del figlio, se c'è e non è vuoto.
fn testo_di(n: Node, nome: &'static str) -> Option<String> {
    figlio(n, nome).map(testo).filter(|t| !t.is_empty())
}

/// «nel pozzetto A1 manca «PlateID»».
fn manca_in(pozzetto: &str, campo: &str) -> String {
    format!("nel pozzetto {pozzetto} manca «{campo}»")
}

fn manca_regione(pozzetto: &str, regione: &str, campo: &str) -> String {
    format!("nella regione {regione} del pozzetto {pozzetto} manca «{campo}»")
}

fn non_numero_regione(pozzetto: &str, regione: &str, campo: &str, t: &str) -> String {
    format!("nella regione {regione} del pozzetto {pozzetto} il valore di «{campo}» non è un numero: «{t}»")
}

fn non_numero(pozzetto: &str, campo: &str, t: &str) -> String {
    format!("nel pozzetto {pozzetto} il valore di «{campo}» non è un numero: «{t}»")
}

/// Scende per i nomi: «RunConditions» e poi «RP1Gain».
fn scendi<'a, 'b>(n: Node<'a, 'b>, percorso: &[&'static str]) -> Option<Node<'a, 'b>> {
    percorso.iter().try_fold(n, |x, nome| figlio(x, nome))
}

fn lettere_riga(r: u32) -> String {
    // 1 → A, 26 → Z, 27 → AA.
    let mut n = r;
    let mut s = String::new();
    while n > 0 {
        let resto = (n - 1) % 26;
        s.insert(0, (b'A' + resto as u8) as char);
        n = (n - 1) / 26;
    }
    s
}

/// «A1» dalla riga e dalla colonna del pozzetto.
fn nome_pozzetto(n: Node) -> Result<String, String> {
    let riga: u32 = n.attribute("RowNo").and_then(|r| r.trim().parse().ok()).filter(|r| *r > 0).ok_or("un pozzetto non ha il numero di riga")?;
    let colonna = n.attribute("ColNo").map(str::trim).filter(|c| !c.is_empty()).ok_or("un pozzetto non ha il numero di colonna")?;
    Ok(format!("{}{}", lettere_riga(riga), colonna))
}

// ---------------------------------------------------------------- i campioni

struct Campione {
    ruolo: Ruolo,
    etichetta: String,
    descrizione: Option<String>,
    diluizione: Option<f64>,
    /// Gli errori di lettura per analita: (messaggio, nome dell'analita).
    errori: Vec<(String, String)>,
    /// Il nome dell'analita per numero di regione.
    regioni: HashMap<String, String>,
    altro: Vec<Valore>,
}

fn campioni(samples: Node) -> Result<HashMap<String, Campione>, String> {
    let mut out = HashMap::new();
    for tipo in samples.children().filter(Node::is_element) {
        for campione in tipo.children().filter(Node::is_element) {
            let ruolo = match campione.tag_name().name() {
                "Blank" => Ruolo::Bianco,
                "Control" => Ruolo::Controllo,
                "Standard" => Ruolo::Standard,
                "Unknown" => Ruolo::Campione,
                altro => return Err(format!("il tipo di campione «{altro}» non è fra quelli che Cedro conosce")),
            };
            let etichetta = testo_di(campione, "Label").ok_or("un campione non ha l'etichetta")?;
            let descrizione = testo_di(campione, "Description");
            let diluizione = testo_di(campione, "Dilution").and_then(|d| numero(&d));
            for gruppo in figli(campione, "MemberWells") {
                for pozzetto in figli(gruppo, "MemberWell") {
                    let nome = nome_pozzetto(pozzetto)?;
                    let mut errori = Vec::new();
                    let mut regioni = HashMap::new();
                    if let Some(mw) = figlio(pozzetto, "MWAnalytes") {
                        for a in figli(mw, "MWAnalyte") {
                            let regione = a.attribute("RegionNumber").ok_or("un analita non ha il numero di regione")?.trim().to_string();
                            let nome_analita = testo_di(a, "AnalyteName").ok_or_else(|| format!("l'analita della regione {regione} non ha il nome"))?;
                            let codice = figlio(a, "Reading").and_then(|r| r.attribute("Code")).ok_or_else(|| format!("l'analita {nome_analita} non ha il codice di lettura"))?.trim();
                            if codice != "0" {
                                let msg = ERRORI.iter().find(|(c, _)| *c == codice).map(|(_, m)| *m).ok_or_else(|| format!("il codice di errore «{codice}» dell'analita {nome_analita} non è fra quelli che Cedro conosce"))?;
                                errori.push((msg.to_string(), nome_analita.clone()));
                            }
                            regioni.insert(regione, nome_analita);
                        }
                    }
                    // Gli attributi del pozzetto del campione (riga e colonna) restano come nota della misura.
                    let altro: Vec<Valore> = pozzetto.attributes().filter(|a| !matches!(a.name(), "WellNumber" | "WellNo")).map(|a| Valore::testo(a.name(), a.value(), Posto::Altro)).collect();
                    out.insert(nome, Campione { ruolo, etichetta: etichetta.clone(), descrizione: descrizione.clone(), diluizione, errori, regioni, altro });
                }
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- la lettura

fn leggi(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    let xml = f.testo.trim_start_matches('\u{feff}');
    let doc = Document::parse_with_options(xml, roxmltree::ParsingOptions { allow_dtd: false, ..Default::default() }).map_err(|e| format!("il file XML non si legge: {e}"))?;
    let radice = doc.root_element();
    let mancanti: Vec<&str> = ["Samples", "NativeDocumentLocation", "Description", "PlateDimensions", "Wells"].into_iter().filter(|t| figlio(radice, t).is_none()).collect();
    if !mancanti.is_empty() {
        return Err(format!("nel file mancano gli elementi: {}", mancanti.join(", ")));
    }
    let campioni = campioni(figlio(radice, "Samples").ok_or_else(|| "nel file mancano gli elementi: Samples".to_string())?)?;
    let pozzetti_xml: Vec<Node> = figli(figlio(radice, "Wells").ok_or_else(|| "nel file mancano gli elementi: Wells".to_string())?, "Well").collect();
    let primo = *pozzetti_xml.first().ok_or("nel file non ci sono pozzetti letti")?;
    let n_pozzetti: u32 = figlio(radice, "PlateDimensions").and_then(|p| p.attribute("TotalWells")).and_then(|n| n.trim().parse().ok()).ok_or("le dimensioni della piastra non dicono quanti pozzetti ha")?;

    // Il sistema: dal primo pozzetto.
    let seriale = scendi(primo, &["MachineInfo", "SerialNumber"]).map(testo).filter(|t| !t.is_empty()).ok_or_else(|| format!("nel primo pozzetto manca «{}»", "SerialNumber"))?;
    let firmware = scendi(primo, &["MachineInfo", "MicroControllerVersion"]).map(testo).filter(|t| !t.is_empty()).ok_or_else(|| format!("nel primo pozzetto manca «{}»", "MicroControllerVersion"))?;
    let metodo = testo_di(primo, "RunProtocolDocumentLocation").ok_or_else(|| format!("nel primo pozzetto manca «{}»", "RunProtocolDocumentLocation"))?;
    let regioni_di_interesse: Vec<String> = scendi(primo, &["RunSettings", "RegionsOfInterest"])
        .map(|r| figli(r, "Region").filter_map(|x| x.attribute("RegionNumber").map(|v| v.trim().to_string())).collect())
        .ok_or_else(|| format!("nel primo pozzetto manca «{}»", "RegionsOfInterest"))?;
    let dato_file = testo_di(radice, "NativeDocumentLocation");
    let tipo_esperimento = testo_di(radice, "Description");

    // Le corse: un gruppo per piastra e utente.
    let mut chiavi: Vec<(String, String)> = Vec::new();
    let mut gruppi: Vec<Gruppo> = Vec::new();
    for xml in &pozzetti_xml {
        let nome = nome_pozzetto(*xml)?;
        let campione = campioni.get(&nome).ok_or_else(|| format!("il pozzetto {nome} non ha un campione"))?;
        let piastra = testo_di(*xml, "PlateID").ok_or_else(|| manca_in(&nome, "PlateID"))?;
        let utente = testo_di(*xml, "User").ok_or_else(|| manca_in(&nome, "User"))?;
        let ora = testo_di(*xml, "AcquisitionTime").ok_or_else(|| manca_in(&nome, "AcquisitionTime"))?;
        let volume_testo = scendi(*xml, &["RunSettings", "SampleVolume"]).map(testo).filter(|t| !t.is_empty()).ok_or_else(|| manca_in(&nome, "SampleVolume"))?;
        let volume = numero(&volume_testo).ok_or_else(|| non_numero(&nome, "SampleVolume", &volume_testo))?;
        let guadagno = scendi(*xml, &["RunConditions", "RP1Gain"]).map(testo).filter(|t| !t.is_empty()).ok_or_else(|| manca_in(&nome, "RP1Gain"))?;
        let criterio = scendi(*xml, &["RunSettings", "StopReadingCriteria"]).ok_or_else(|| manca_in(&nome, "StopReadingCriteria"))?;
        let sfere_testo = criterio.attribute("BeadCount").map(str::trim).filter(|t| !t.is_empty()).ok_or_else(|| manca_in(&nome, "BeadCount"))?;
        let sfere_minime = numero(sfere_testo).ok_or_else(|| non_numero(&nome, "BeadCount", sfere_testo))?;
        let eventi_testo = testo_di(*xml, "TotalEvents").ok_or_else(|| manca_in(&nome, "TotalEvents"))?;
        let eventi = numero(&eventi_testo).ok_or_else(|| non_numero(&nome, "TotalEvents", &eventi_testo))?;

        let chiave = (piastra.clone(), utente.clone());
        let gi = match chiavi.iter().position(|k| *k == chiave) {
            Some(i) => i,
            None => {
                let mut g = Gruppo { titolo: piastra.clone(), quando: tempo::iso(&ora, false), piastra: Some(piastra.clone()), pozzetti: Some(n_pozzetti), ..Default::default() };
                g.dati.push(("analyst".into(), utente.clone()));
                g.dati.push(("analytical method identifier".into(), metodo.clone()));
                if let Some(d) = &dato_file {
                    g.dati.push(("experimental data identifier".into(), d.clone()));
                }
                if let Some(t) = &tipo_esperimento {
                    g.dati.push(("experiment type".into(), t.clone()));
                }
                if let Some(b) = criterio.attribute("BeadCountIn") {
                    g.dati.push(("~BeadCountIn".into(), b.trim().to_string()));
                }
                chiavi.push(chiave);
                gruppi.push(g);
                gruppi.len() - 1
            }
        };

        let mut m = Misura { id: l.id(), campione: campione.etichetta.clone(), pozzetto: Some(nome.clone()), piastra: Some(piastra.clone()), ruolo: campione.ruolo, ruolo_dal_file: true, quando: tempo::iso(&ora, false), ..Default::default() };
        if let Some(d) = &campione.descrizione {
            m.valori.push(Valore::testo("description", d, Posto::Campione));
        }
        m.valori.push(Valore::testo("detector gain setting", &guadagno, Posto::Impostazione));
        m.valori.push(Valore::num("sample volume setting", volume, "μL", Posto::Impostazione));
        if let Some(d) = campione.diluizione {
            m.valori.push(Valore::num("dilution factor setting", d, "(unitless)", Posto::Impostazione));
        }
        m.valori.push(Valore::num("minimum assay bead count threshold setting", sfere_minime, "#", Posto::Impostazione));
        m.valori.push(Valore::num("assay bead count", eventi, "#", Posto::Misura));
        m.valori.extend(campione.altro.iter().cloned());
        for tag in ["TotalGatedEvents", "TotalRegionEventCount", "RunProtocolDocumentName"] {
            if let Some(t) = testo_di(*xml, tag) {
                m.valori.push(Valore::testo(tag, &t, Posto::Altro));
            }
        }
        m.note.extend(campione.errori.iter().cloned());

        // Le sfere per regione di interesse, con le statistiche.
        if let Some(regioni) = figlio(*xml, "BeadRegions") {
            for regione in figli(regioni, "BeadRegion") {
                let numero_regione = regione.attribute("RegionNumber").map(str::trim).unwrap_or("").to_string();
                if !regioni_di_interesse.contains(&numero_regione) {
                    continue;
                }
                let nome_analita = campione.regioni.get(&numero_regione).cloned().ok_or_else(|| format!("la regione {numero_regione} del pozzetto {nome} non ha un analita nel campione"))?;
                let conteggio_testo = testo_di(regione, "RegionCount").ok_or_else(|| manca_regione(&nome, &numero_regione, "RegionCount"))?;
                let conteggio = numero(&conteggio_testo).ok_or_else(|| non_numero_regione(&nome, &numero_regione, "RegionCount", &conteggio_testo))?;
                let mut statistiche = Vec::new();
                for (tag, ruolo, unita) in STATISTICHE {
                    let t = testo_di(regione, tag).ok_or_else(|| manca_regione(&nome, &numero_regione, tag))?;
                    let v = numero(&t).ok_or_else(|| non_numero_regione(&nome, &numero_regione, tag, &t))?;
                    statistiche.push(Statistica { ruolo: (*ruolo).into(), valore: v, unita: (*unita).into() });
                }
                // Il resto della regione (errore standard, CV del troncato…) resta com'è scritto.
                let letti = ["RegionCount", "Median", "Mean", "CV", "StdDev", "TrimmedMean", "TrimmedStdDev"];
                let altro: Vec<Valore> = regione
                    .children()
                    .filter(|c| c.is_element() && !letti.contains(&c.tag_name().name()))
                    .filter_map(|c| Some(testo(c)).filter(|t| !t.is_empty()).map(|t| Valore::testo(c.tag_name().name(), &t, Posto::Altro)))
                    .collect();
                m.analiti.push(Analita { id: l.id(), nome: nome_analita, sfera: Some(numero_regione), conteggio: Some(conteggio), statistiche, calcolati: Vec::new(), altro });
            }
        }
        gruppi[gi].misure.push(m);
    }

    l.strumento.produttore = "Bio-Rad".into();
    l.strumento.seriale = seriale;
    l.strumento.firmware = firmware;
    l.strumento.software = "Bio-Plex Manager".into();
    l.strumento.versione = radice.attribute("BioPlexManagerVersion").unwrap_or("").to_string();
    // Le informazioni del file che nessuno ha usato: l'autore, il nome del documento, chi e quando l'ha esportato.
    for c in radice.children().filter(|c| c.is_element() && c.children().all(|x| !x.is_element())) {
        let nome = c.tag_name().name();
        if matches!(nome, "NativeDocumentLocation" | "Description") {
            continue;
        }
        let t = testo(c);
        if !t.is_empty() {
            l.dispositivo.push(Valore::testo(nome, &t, Posto::Altro));
        }
    }
    for a in radice.attributes().filter(|a| a.name() != "BioPlexManagerVersion") {
        l.dispositivo.push(Valore::testo(a.name(), a.value(), Posto::Altro));
    }
    for (etichetta, v) in [("Documento", radice.children().find(|c| c.is_element() && c.tag_name().name() == "NativeDocumentName").map(testo)), ("Protocollo", Some(metodo)), ("Esportato il", radice.attribute("XmlExportDateTime").map(str::to_string)), ("Esportato da", radice.attribute("XmlExportUser").map(str::to_string))] {
        if let Some(v) = v.filter(|v| !v.is_empty()) {
            l.info.push((etichetta.into(), v));
        }
    }
    l.principale = "assay bead count".into();
    l.unita = "#".into();
    l.gruppi.extend(gruppi);
    Ok(())
}
