//! Una tabella letta per nome di colonna, per i profili: le colonne si chiedono per nome (senza badare
//! a maiuscole e spazi) e si ricorda quali sono state lette, perché le altre diventano note.

use std::cell::RefCell;
use std::collections::HashSet;

use crate::strumenti::fonte::numero;

pub(super) fn norma(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

/// La tabella di un file: le colonne per nome, e quali sono state lette (le altre diventano note).
pub(super) struct Tabella {
    pub testa: Vec<String>,
    pub nomi: Vec<String>,
    pub usate: RefCell<HashSet<usize>>,
}

impl Tabella {
    pub fn nuova(testa: &[String]) -> Tabella {
        Tabella { testa: testa.iter().map(|s| s.trim().to_string()).collect(), nomi: testa.iter().map(|s| norma(s)).collect(), usate: RefCell::new(HashSet::new()) }
    }

    pub fn indice(&self, nome: &str) -> Option<usize> {
        let n = norma(nome);
        self.nomi.iter().position(|c| *c == n)
    }

    pub fn ha(&self, nome: &str) -> bool {
        self.indice(nome).is_some()
    }

    /// Il testo della cella della prima colonna che c'è tra questi nomi (vuoto = nessuno).
    pub fn cella<'a>(&self, r: &'a [String], nomi: &[&str]) -> Option<&'a str> {
        let i = nomi.iter().find_map(|n| self.indice(n))?;
        self.usate.borrow_mut().insert(i);
        r.get(i).map(|s| s.trim()).filter(|s| !s.is_empty())
    }

    /// Come `cella`, ma prova ogni nome e prende il primo che ha un testo.
    pub fn prima<'a>(&self, r: &'a [String], nomi: &[&str]) -> Option<&'a str> {
        nomi.iter().find_map(|n| self.cella(r, &[n]))
    }

    pub fn num(&self, r: &[String], nomi: &[&str]) -> Option<f64> {
        self.cella(r, nomi).and_then(numero)
    }

    /// Segna delle colonne come già lette.
    pub fn leggi(&self, nomi: &[&str]) {
        for n in nomi {
            if let Some(i) = self.indice(n) {
                self.usate.borrow_mut().insert(i);
            }
        }
    }

    /// Le colonne piene che nessuno ha letto, con il loro nome.
    pub fn avanzi(&self, r: &[String]) -> Vec<(String, String)> {
        let usate = self.usate.borrow();
        self.testa.iter().enumerate().filter(|(i, h)| !usate.contains(i) && !h.is_empty()).filter_map(|(i, h)| r.get(i).map(|c| c.trim()).filter(|c| !c.is_empty()).map(|c| (h.clone(), c.to_string()))).collect()
    }
}

