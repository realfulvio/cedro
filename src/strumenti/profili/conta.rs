//! I valori di un contacellule come li vuole Allotrope (cell-counting): vitalità e densità, conteggi,
//! diametri, e le impostazioni di conteggio. Ogni profilo riempie quelli che il suo file ha.

use crate::strumenti::{Dato, Misura, Posto, Valore};

#[derive(Default)]
pub(super) struct Conta {
    pub tipo_cellula: Option<String>,
    pub diam_min: Option<f64>,
    pub diam_max: Option<f64>,
    pub diluizione: Option<f64>,
    /// Volume del campione (µL).
    pub volume: Option<f64>,
    /// Vitalità (%).
    pub vitalita: Option<f64>,
    /// Densità in 10^6 cellule/mL.
    pub densita_vive: Option<f64>,
    pub densita_morte: Option<f64>,
    pub densita_totale: Option<f64>,
    pub totale: Option<f64>,
    pub vive: Option<f64>,
    pub morte: Option<f64>,
    pub diam_medio: Option<f64>,
    pub diam_medio_vive: Option<f64>,
    pub diam_medio_morte: Option<f64>,
    pub circ_media: Option<f64>,
    pub circ_media_vive: Option<f64>,
}

impl Conta {
    /// Mette i valori nella misura: il rivelatore e il volume fra le impostazioni, il tipo di cellula,
    /// i diametri e la diluizione fra quelle del conteggio, i numeri fra i dati elaborati.
    pub fn applica(&self, m: &mut Misura) {
        m.valori.push(Valore::testo("detection type", "brightfield", Posto::Impostazione));
        if let Some(v) = self.volume {
            m.valori.push(Valore::num("sample volume setting", v, "μL", Posto::Impostazione));
        }
        if let Some(t) = &self.tipo_cellula {
            m.valori.push(Valore::testo("cell type processing method", t, Posto::Elaborazione));
        }
        for (k, v, u) in [("minimum cell diameter setting", self.diam_min, "µm"), ("maximum cell diameter setting", self.diam_max, "µm"), ("cell density dilution factor", self.diluizione, "")] {
            if let Some(v) = v {
                m.valori.push(Valore::num(k, v, u, Posto::Elaborazione));
            }
        }
        // Vitalità e densità delle vive sono obbligatorie nello schema: se il file non le ha, -0 e la nota.
        for (k, v, u) in [("viability (cell counter)", self.vitalita, "%"), ("viable cell density (cell counter)", self.densita_vive, "10^6 cells/mL")] {
            m.valori.push(match v {
                Some(v) => Valore::num(k, v, u, Posto::Elaborato),
                None => Valore { chiave: k.into(), dato: Dato::Assente(String::new()), unita: u.into(), posto: Posto::Elaborato },
            });
        }
        for (k, v, u) in [
            ("dead cell density (cell counter)", self.densita_morte, "10^6 cells/mL"),
            ("total cell density (cell counter)", self.densita_totale, "10^6 cells/mL"),
            ("total cell count", self.totale.map(f64::round), "cell"),
            ("viable cell count", self.vive.map(f64::round), "cell"),
            ("dead cell count", self.morte.map(f64::round), "cell"),
            ("average total cell diameter", self.diam_medio, "µm"),
            ("average live cell diameter (cell counter)", self.diam_medio_vive, "µm"),
            ("average dead cell diameter (cell counter)", self.diam_medio_morte, "µm"),
            ("average total cell circularity", self.circ_media, ""),
            ("average viable cell circularity", self.circ_media_vive, ""),
        ] {
            if let Some(v) = v {
                m.valori.push(Valore::num(k, v, u, Posto::Elaborato));
            }
        }
    }
}

/// Un valore di nota: numero se lo è, testo altrimenti.
pub(super) fn nota(h: &str, c: &str, posto: Posto) -> Valore {
    match crate::strumenti::fonte::numero(c) {
        Some(v) => Valore { chiave: h.to_string(), dato: Dato::Numero(v), unita: String::new(), posto },
        None => Valore::testo(h, c, posto),
    }
}

/// Mette fra le note della misura le colonne di questo elenco che il file ha, con il nome scritto nel file.
pub(super) fn custom(t: &super::tabella::Tabella, r: &[String], nomi: &[&str], posto: Posto, m: &mut Misura) {
    for n in nomi {
        if let (Some(i), Some(c)) = (t.indice(n), t.cella(r, &[n])) {
            m.valori.push(nota(&t.testa[i], c, posto));
        }
    }
}
