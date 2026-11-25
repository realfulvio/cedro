//! Thermo Fisher VISIONlite e Genesys On-Board: il CSV (spesso in UTF-16, con virgole o tabulazioni)
//! di tre tipi di prova: «Fixed» (assorbanza a una o più lunghezze d'onda), «Quant» (assorbanza e
//! concentrazione con il fattore di diluizione) e «Scan» (uno spettro, con una riga di intestazione
//! prima della tabella «nm, A»). Il Genesys On-Board scrive lo stesso formato, con «Well Position».

use super::tabella::Tabella;
use crate::strumenti::fonte::{numero, Fonte};
use crate::strumenti::{tempo, Calcolato, Dato, Gruppo, Lettura, Misura, Posto, Profilo, Tecnica, Valore};

pub const VISIONLITE: Profilo = Profilo {
    id: "thermo_fisher_visionlite",
    nome: "Thermo Fisher VISIONlite",
    produttore: "Thermo Fisher Scientific",
    tecnica: Tecnica::Spettrofotometria,
    estensioni: &["csv"],
    riconosci: riconosci_visionlite,
    leggi: leggi_visionlite,
};

pub const GENESYS_ON_BOARD: Profilo = Profilo {
    id: "thermo_fisher_genesys_on_board",
    nome: "Thermo Fisher Genesys On-Board",
    produttore: "Thermo Fisher Scientific",
    tecnica: Tecnica::Spettrofotometria,
    estensioni: &["csv"],
    riconosci: riconosci_genesys,
    leggi: leggi_genesys,
};

fn righe(f: &Fonte) -> Vec<Vec<String>> {
    let sep = if f.testo.lines().next().is_some_and(|r| r.contains('\t')) { '\t' } else { ',' };
    f.righe_con(sep).into_iter().filter(|r| r.iter().any(|c| !c.trim().is_empty())).collect()
}

fn prima_cella(r: &[Vec<String>]) -> String {
    r.first().and_then(|r| r.first()).map(|c| c.trim().to_lowercase()).unwrap_or_default()
}

fn e_scansione(r: &[Vec<String>]) -> bool {
    r.get(1).and_then(|r| r.first()).is_some_and(|c| c.trim() == "nm")
}

fn riconosci_visionlite(f: &Fonte) -> u8 {
    let r = righe(f);
    if prima_cella(&r).starts_with("sample name") || e_scansione(&r) {
        80
    } else {
        0
    }
}

fn riconosci_genesys(f: &Fonte) -> u8 {
    let r = righe(f);
    if prima_cella(&r).starts_with("well position") {
        85
    } else {
        0
    }
}

fn leggi_visionlite(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    leggi(f, l, "VISIONlite")
}

fn leggi_genesys(f: &Fonte, l: &mut Lettura) -> Result<(), String> {
    leggi(f, l, "Genesys On-Board")
}

/// «A260» o «500 nm   [A]» → la lunghezza d'onda.
fn lunghezza_onda(h: &str) -> Option<f64> {
    let h = h.trim();
    if let Some(c) = h.strip_prefix('A') {
        if !c.is_empty() && c.bytes().all(|b| b.is_ascii_digit()) {
            return c.parse().ok();
        }
    }
    let p: Vec<&str> = h.split_whitespace().collect();
    if p.len() == 3 && p[1] == "nm" && p[2] == "[A]" {
        return p[0].parse().ok();
    }
    None
}

fn leggi(f: &Fonte, l: &mut Lettura, software: &str) -> Result<(), String> {
    let r = righe(f);
    if r.is_empty() {
        return Err("file vuoto".into());
    }
    l.strumento.produttore = "Thermo Fisher Scientific".into();
    l.strumento.software = software.into();
    l.principale = "absorbance".into();
    l.unita = "AU".into();
    if prima_cella(&r).starts_with("sample name") || prima_cella(&r).starts_with("well position") {
        leggi_tabella(&r, l)
    } else if e_scansione(&r) {
        leggi_scansione(&r, l)
    } else {
        Err("il file non è una prova «Fixed», «Quant» o «Scan»".into())
    }
}

/// «Scan»: una riga di intestazione (file, campione, analista, ora…) e la tabella nm, A.
fn leggi_scansione(r: &[Vec<String>], l: &mut Lettura) -> Result<(), String> {
    let testa = &r[0];
    let cella = |i: usize| testa.get(i).map(|s| s.trim().to_string()).filter(|s| !s.is_empty());
    let campione = cella(1).ok_or("manca il nome del campione nell'intestazione")?;
    let spettro: Vec<(f64, f64)> = r[2..].iter().filter_map(|x| Some((numero(x.first()?)?, numero(x.get(1)?)?))).collect();
    if spettro.is_empty() {
        return Err("nello spettro non ci sono punti".into());
    }
    let quando = cella(3).and_then(|d| tempo::iso(&d, tempo::giorno_prima([d.as_str()])));
    let mut g = Gruppo { titolo: "Scan".into(), quando: quando.clone(), ..Default::default() };
    g.dati.push(("experiment type".into(), "Scan".into()));
    if let Some(a) = cella(2) {
        g.dati.push(("analyst".into(), a));
    }
    if let Some(n) = cella(0) {
        l.info.push(("File della scansione".into(), n));
    }
    let altri: Vec<String> = (4..testa.len()).filter_map(cella).collect();
    if !altri.is_empty() {
        l.info.push(("Altri dati dell'intestazione".into(), altri.join(" · ")));
    }
    let mut m = Misura { campione, quando, spettro, ..Default::default() };
    m.valori.push(Valore::testo("detection type", "absorbance", Posto::Impostazione));
    g.misure.push(m);
    l.gruppi.push(g);
    Ok(())
}

/// «Fixed» e «Quant»: una riga per campione.
fn leggi_tabella(r: &[Vec<String>], l: &mut Lettura) -> Result<(), String> {
    let t = Tabella::nuova(&r[0]);
    let quant = t.ha("dilution factor");
    let tipo = if quant { "Quant" } else { "Fixed" };
    let onde: Vec<(usize, f64)> = t.testa.iter().enumerate().filter_map(|(i, h)| lunghezza_onda(h).map(|w| (i, w))).collect();
    let concentrazione = t.testa.iter().enumerate().find_map(|(i, h)| h.strip_prefix("Concentration [").and_then(|x| x.strip_suffix(']')).map(|u| (i, u.to_string())));
    let mut g = Gruppo { titolo: tipo.into(), ..Default::default() };
    g.dati.push(("experiment type".into(), tipo.into()));
    for riga in &r[1..] {
        let Some(campione) = t.prima(riga, &["sample name", "well position"]) else { continue };
        let mut misure: Vec<Misura> = Vec::new();
        let assorbanze: Vec<(Option<f64>, Option<f64>)> = if quant {
            let v = t.num(riga, &["ordinate [a]"]);
            if !t.ha("ordinate [a]") {
                return Err("nella prova Quant manca la colonna «Ordinate [A]»".into());
            }
            vec![(None, v)]
        } else if !onde.is_empty() {
            onde.iter().map(|&(i, w)| (Some(w), riga.get(i).and_then(|c| numero(c)))).collect()
        } else if t.ha("absorbance") {
            vec![(None, t.num(riga, &["absorbance"]))]
        } else {
            return Err("nella prova Fixed non ci sono colonne di assorbanza (A260, «500 nm [A]», Absorbance)".into());
        };
        for &(i, _) in &onde {
            t.usate.borrow_mut().insert(i);
        }
        if let Some((i, _)) = &concentrazione {
            t.usate.borrow_mut().insert(*i);
        }
        let conc = concentrazione.as_ref().and_then(|(i, u)| riga.get(*i).and_then(|c| numero(c)).map(|v| (v, u.clone())));
        let diluizione = t.num(riga, &["dilution factor"]);
        let risultato = t.num(riga, &["result"]);
        let avanzi = t.avanzi(riga);
        for (w, a) in assorbanze {
            let mut m = Misura { id: l.id(), campione: campione.to_string(), ..Default::default() };
            m.valori.push(Valore::testo("detection type", "absorbance", Posto::Impostazione));
            if let Some(w) = w {
                m.valori.push(Valore::num("detector wavelength setting", w, "nm", Posto::Impostazione));
            }
            if let Some(d) = diluizione {
                m.valori.push(Valore::num("dilution factor", d, "", Posto::ImpostazioneAltro));
            }
            m.valori.push(match a {
                Some(v) => Valore::num("absorbance", v, "AU", Posto::Misura),
                None => Valore { chiave: "absorbance".into(), dato: Dato::Assente(String::new()), unita: "AU".into(), posto: Posto::Misura },
            });
            if let Some((c, u)) = &conc {
                m.valori.push(Valore::num("mass concentration", *c, u, Posto::Elaborato));
            }
            for (h, c) in &avanzi {
                m.valori.push(match numero(c) {
                    Some(v) => Valore { chiave: h.clone(), dato: Dato::Numero(v), unita: String::new(), posto: Posto::Altro },
                    None => Valore::testo(h, c, Posto::Altro),
                });
            }
            misure.push(m);
        }
        if let Some(v) = risultato.filter(|v| *v != 0.0) {
            l.calcolati.push(Calcolato { nome: "Result".into(), valore: v, unita: String::new(), sorgenti: misure.iter().map(|m| m.id.clone()).collect(), da: "absorbance".into() });
        }
        g.misure.extend(misure);
    }
    if g.misure.is_empty() {
        return Err("nel file non ci sono campioni".into());
    }
    l.gruppi.push(g);
    Ok(())
}
