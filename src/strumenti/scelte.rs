//! Le scelte di chi converte un file di strumento e quello che ne esce: servono all'azione «strumento»
//! del motore e alla finestra di Cedro 4 Labs. Qui non si tocca il disco: i nomi dei file li sceglie il
//! motore (con le sue regole per i nomi che esistono già), qui si preparano i contenuti.

use super::{asm, excel, Dato, Fonte, Lettura, Ruolo};

/// Le scelte per un file: lo strumento (se non va quello riconosciuto), i ruoli segnati sui pozzetti,
/// le uscite e le opzioni di Allotrope. Solo testi e sì/no: sta dentro le `Options` del motore.
#[derive(Clone, PartialEq, Eq, Debug)]
#[cfg_attr(feature = "android-worker", derive(serde::Serialize, serde::Deserialize))]
pub struct Scelte {
    /// L'identificativo del profilo scelto a mano; vuoto = quello che Cedro riconosce.
    pub profilo: String,
    /// I ruoli segnati sui pozzetti: (pozzetto «A1», lettera B, S, C o R). Vincono su quelli del file.
    pub ruoli: Vec<(String, char)>,
    /// La mappa dell'Excel mostra i valori meno la media dei bianchi.
    pub meno_bianco: bool,
    /// Scala di colori per chi non distingue il rosso dal blu.
    pub daltonici: bool,
    /// L'Excel (Mappa, Pozzetti o Campioni, Spettri, Strumento): è l'uscita di base.
    pub excel: bool,
    /// Il CSV della tabella dei valori.
    pub csv: bool,
    /// Allotrope (ASM), per chi ha un LIMS o un archivio dati.
    pub asm: bool,
    /// Il CSV piatto di Allotrope: una riga per misura.
    pub piatto: bool,
    /// Allotrope con i calcoli di Cedro (bianco, valori meno il bianco), separati dai dati grezzi.
    pub asm_calcoli: bool,
    /// Allotrope con il percorso completo del file originale (altrimenti solo il nome).
    pub asm_percorso: bool,
    /// Il file Allotrope scritto si controlla prima di salvarlo.
    pub asm_controlla: bool,
}

impl Default for Scelte {
    fn default() -> Scelte {
        Scelte { profilo: String::new(), ruoli: Vec::new(), meno_bianco: false, daltonici: false, excel: true, csv: false, asm: false, piatto: false, asm_calcoli: false, asm_percorso: false, asm_controlla: true }
    }
}

impl Scelte {
    /// I ruoli con il tipo vero.
    pub fn ruoli_veri(&self) -> Vec<(String, Ruolo)> {
        self.ruoli.iter().filter_map(|(w, c)| Some((w.clone(), ruolo_da_lettera(*c)?))).collect()
    }

    /// Segna (o toglie, con `Campione`) il ruolo di un pozzetto.
    pub fn segna(&mut self, pozzetto: &str, ruolo: Ruolo) {
        let rc = super::pozzetto_rc(pozzetto);
        self.ruoli.retain(|(w, _)| super::pozzetto_rc(w) != rc);
        let l = ruolo.lettera();
        if let Some(c) = l.chars().next() {
            self.ruoli.push((pozzetto.to_string(), c));
        }
    }

    /// Almeno un'uscita è accesa.
    pub fn esce_qualcosa(&self) -> bool {
        self.excel || self.csv || self.asm || self.piatto
    }

    pub fn excel_opz(&self) -> excel::Opzioni {
        excel::Opzioni { ruoli: self.ruoli_veri(), meno_bianco: self.meno_bianco, daltonici: self.daltonici }
    }

    pub fn asm_opz(&self, origine: &str, nome_asm: &str) -> asm::Opzioni {
        asm::Opzioni { ruoli: self.ruoli_veri(), calcoli: self.asm_calcoli, percorso: self.asm_percorso.then(|| origine.to_string()), nome_asm: nome_asm.to_string() }
    }
}

pub fn ruolo_da_lettera(c: char) -> Option<Ruolo> {
    Some(match c.to_ascii_uppercase() {
        'B' => Ruolo::Bianco,
        'S' => Ruolo::Standard,
        'C' => Ruolo::Controllo,
        'R' => Ruolo::Riferimento,
        _ => return None,
    })
}

/// Il file letto con il profilo scelto (vuoto = quello che lo riconosce).
pub fn leggi_scelto(fonte: &Fonte, profilo: &str) -> Result<Lettura, String> {
    if profilo.is_empty() {
        return super::leggi(fonte);
    }
    let p = super::profilo(profilo).ok_or_else(|| format!("profilo «{profilo}» sconosciuto"))?;
    super::leggi_con(p, fonte)
}

/// Quello che esce da una lettura, in memoria: i nomi e la scrittura su disco sono del motore.
#[derive(Clone, Debug, Default)]
pub struct Uscite {
    pub excel: Option<Vec<u8>>,
    pub csv: Option<String>,
    pub asm: Option<Vec<u8>>,
    pub piatto: Option<String>,
    /// Allotrope controllato: `Some(Ok)` passa, `Some(Err)` dice cosa non va.
    pub controllo: Option<Result<(), String>>,
}

/// Prepara i contenuti delle uscite scelte. `origine` è il percorso del file letto, `nome_asm` il nome
/// del file Allotrope che si scrive (entra nel «data system document»); `sep` il separatore del CSV.
pub fn prepara(l: &Lettura, s: &Scelte, origine: &str, nome_asm: &str, sep: char) -> Result<Uscite, String> {
    let mut u = Uscite::default();
    if s.excel {
        u.excel = Some(excel::scrivi(l, &s.excel_opz())?);
    }
    if s.csv {
        u.csv = Some(excel::csv(l, &s.excel_opz(), sep));
    }
    if s.asm || s.piatto {
        let o = s.asm_opz(origine, nome_asm);
        let v = asm::scrivi(l, &o);
        if s.asm_controlla {
            u.controllo = Some(asm::controlla(&v));
            if let Some(Err(e)) = &u.controllo {
                if s.asm {
                    return Err(format!("il file Allotrope non passa il controllo: {e}"));
                }
            }
        }
        if s.asm {
            u.asm = Some(serde_json::to_vec_pretty(&v).map_err(|e| e.to_string())?);
        }
        if s.piatto {
            u.piatto = Some(asm::piatto(l, &o));
        }
    }
    Ok(u)
}

/// «24/11/2026 14:05» da una data ISO («2026-11-24T14:05:00+00:00»).
pub fn data_it(iso: &str) -> String {
    let t = iso.replace('T', " ");
    let (d, h) = t.split_once(' ').unwrap_or((t.as_str(), ""));
    let mut p = d.split('-');
    let (Some(y), Some(m), Some(g)) = (p.next(), p.next(), p.next()) else { return iso.to_string() };
    if y.len() != 4 || m.len() != 2 || g.len() < 2 {
        return iso.to_string();
    }
    let g = &g[..2];
    let h: String = h.chars().take(5).collect();
    if h.is_empty() {
        format!("{g}/{m}/{y}")
    } else {
        format!("{g}/{m}/{y} {h}")
    }
}

/// La data della lettura: quella del primo gruppo o della prima misura che ce l'ha.
pub fn quando(l: &Lettura) -> Option<String> {
    l.gruppi.iter().find_map(|g| g.quando.clone()).or_else(|| l.misure().find_map(|m| m.quando.clone())).map(|q| data_it(&q))
}

/// Il nome dello strumento per le persone: «Tecan Magellan · Sunrise».
pub fn nome_strumento(l: &Lettura) -> String {
    let m = l.strumento.modello.trim();
    if m.is_empty() || l.nome.to_lowercase().contains(&m.to_lowercase()) {
        l.nome.to_string()
    } else {
        format!("{} · {m}", l.nome)
    }
}

/// La riga che dice cosa si è letto: «Lettore di piastre · Piastra 1 · 24/11/2026 14:05».
pub fn riga_lettura(l: &Lettura) -> String {
    riga_lettura_con(l, &|s| s.to_string())
}

/// La stessa riga per la finestra, nella lingua scelta: i pezzi di Cedro si traducono uno per uno, quelli che
/// vengono dal file (il titolo della piastra, la data) restano com'erano. Chi la disegna la segna tutta come
/// testo dell'utente, perché non si traduca una seconda volta.
pub fn riga_lettura_ui(l: &Lettura) -> String {
    riga_lettura_con(l, &|s| crate::i18n::tr_string(s))
}

fn riga_lettura_con(l: &Lettura, di_cedro: &dyn Fn(&str) -> String) -> String {
    let mut p: Vec<String> = vec![di_cedro(l.tecnica.nome())];
    if l.gruppi.len() == 1 && !l.gruppi[0].titolo.is_empty() {
        p.push(l.gruppi[0].titolo.clone());
    } else if l.gruppi.len() > 1 {
        p.push(di_cedro(&format!("{} gruppi", l.gruppi.len())));
    }
    let n = l.conta();
    p.push(di_cedro(&if n == 1 { "1 misura".to_string() } else { format!("{n} misure") }));
    if let Some(q) = quando(l) {
        p.push(q);
    }
    p.join(" · ")
}

/// Una cosa da sapere a lavoro finito.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Nota {
    pub titolo: String,
    pub testo: String,
    /// Va guardata: sta in ambra.
    pub attenzione: bool,
}

/// Le note per chi ha convertito: i pozzetti senza numero, gli avvisi del profilo, il controllo di Allotrope.
pub fn note(l: &Lettura, u: &Uscite) -> Vec<Nota> {
    let mut v: Vec<Nota> = Vec::new();
    // Misure il cui valore principale non è un numero («Undetermined», «OVER»).
    let mut senza: Vec<(String, usize)> = Vec::new();
    for m in l.misure().filter(|m| !m.e_spettro()) {
        if l.valore(m).is_none() {
            let t = match m.valore(&l.principale).map(|x| &x.dato) {
                Some(Dato::Testo(t)) | Some(Dato::Assente(t)) if !t.is_empty() => t.clone(),
                _ => continue,
            };
            match senza.iter_mut().find(|(x, _)| *x == t) {
                Some(e) => e.1 += 1,
                None => senza.push((t, 1)),
            }
        }
    }
    for (t, n) in senza {
        // Il testo del file (per esempio «Undetermined») è dell'utente: la traduzione non lo tocca.
        let dato = crate::i18n::lit(&t);
        let titolo = if l.a_piastra() { format!("{n} {} «{dato}».", if n == 1 { "pozzetto" } else { "pozzetti" }) } else { format!("{n} {} «{dato}».", if n == 1 { "misura" } else { "misure" }) };
        v.push(Nota { titolo, testo: format!("Nell'Excel restano «{dato}», non diventano zero."), attenzione: false });
    }
    for a in &l.avvisi {
        v.push(Nota { titolo: "Da sapere.".into(), testo: a.clone(), attenzione: true });
    }
    match &u.controllo {
        Some(Ok(())) if u.asm.is_some() => v.push(Nota { titolo: "File Allotrope controllato.".into(), testo: "Chiavi, unità e identificativi passano il controllo.".into(), attenzione: false }),
        Some(Err(e)) => v.push(Nota { titolo: "Il file Allotrope non passa il controllo.".into(), testo: e.clone(), attenzione: true }),
        _ if u.asm.is_some() => v.push(Nota { titolo: "File Allotrope scritto.".into(), testo: "Non controllato: la spunta è tolta.".into(), attenzione: false }),
        _ => {}
    }
    v
}
