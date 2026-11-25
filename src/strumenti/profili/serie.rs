//! Una riga di tabella come la legge pandas in allotropy (`SeriesData`): le colonne si chiedono per
//! nome e si ricorda quali sono state lette, perché le altre diventano le note della misura.

use std::cell::RefCell;

use crate::strumenti::fonte::numero;
use crate::strumenti::Dato;

/// Una riga: i nomi delle colonne (già puliti) e le celle. Una cella `None` manca (la riga è più corta),
/// una cella `Some("")` è vuota.
pub(super) struct Serie<'a> {
    nomi: &'a [String],
    celle: &'a [Option<String>],
    letti: RefCell<Vec<bool>>,
}

impl<'a> Serie<'a> {
    pub fn nuova(nomi: &'a [String], celle: &'a [Option<String>]) -> Serie<'a> {
        Serie { nomi, celle, letti: RefCell::new(vec![false; nomi.len()]) }
    }

    /// Segna come letta la colonna (tutte quelle con lo stesso nome).
    pub fn segna(&self, k: &str) {
        let mut l = self.letti.borrow_mut();
        for (i, n) in self.nomi.iter().enumerate() {
            if n == k {
                l[i] = true;
            }
        }
    }

    /// Il testo della cella della colonna, se la riga ce l'ha (anche vuoto).
    pub fn grezzo(&self, k: &str) -> Option<&'a str> {
        self.segna(k);
        let i = self.nomi.iter().position(|n| n == k)?;
        self.celle.get(i)?.as_deref()
    }

    /// Il testo della prima colonna che c'è tra queste (come `get(str, chiavi)`).
    pub fn testo(&self, ks: &[&str]) -> Option<String> {
        ks.iter().find_map(|k| self.grezzo(k).map(str::to_string))
    }

    /// Il testo di una colonna se non è vuoto (come `get(str, k) or …`).
    pub fn testo_pieno(&self, ks: &[&str]) -> Option<String> {
        ks.iter().find_map(|k| self.grezzo(k).filter(|t| !t.trim().is_empty()).map(str::to_string))
    }

    /// Il primo numero che c'è tra queste colonne.
    pub fn num(&self, ks: &[&str]) -> Option<f64> {
        ks.iter().find_map(|k| self.grezzo(k).and_then(numero))
    }

    /// Le colonne che nessuno ha letto e che `salta` non esclude: il numero se è un numero, altrimenti il
    /// testo; le celle mancanti e quelle vuote no.
    pub fn non_letti(&self, salta: &dyn Fn(&str) -> bool) -> Vec<(String, Dato)> {
        let l = self.letti.borrow().clone();
        let mut out = Vec::new();
        for (i, n) in self.nomi.iter().enumerate() {
            if l[i] || salta(n) {
                continue;
            }
            let Some(Some(v)) = self.celle.get(i) else { continue };
            if v.trim().is_empty() {
                continue;
            }
            if out.iter().any(|(k, _): &(String, Dato)| k == n) {
                continue;
            }
            out.push((n.clone(), numero(v).map_or_else(|| Dato::Testo(v.clone()), Dato::Numero)));
        }
        out
    }
}

/// Un nome che comincia con `pre` e continua con una o più cifre (e poi qualsiasi cosa o niente).
pub(super) fn con_cifre(k: &str, pre: &str) -> bool {
    k.strip_prefix(pre).is_some_and(|r| r.chars().next().is_some_and(|c| c.is_ascii_digit()))
}
