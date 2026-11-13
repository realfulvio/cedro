//! I componenti che Cedro scarica al primo uso (yt-dlp, ffmpeg, deno, motori e modelli):
//! chi li scarica o aggiorna lo dice qui, la riga di stato in alto lo mostra.

use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
pub enum Fase {
    /// Byte arrivati e totale (0 se non si sa).
    Scarica { fatti: u64, totale: u64 },
    Aggiorna,
    Errore,
}

#[derive(Clone, Debug)]
pub struct Stato {
    /// Come lo legge una persona: «yt-dlp», «il modello», «il motore».
    pub nome: String,
    pub fase: Fase,
    pub da: Instant,
}

/// Quanto resta visibile un download non riuscito.
const ERRORE_PER: Duration = Duration::from_secs(12);

static ORA: Mutex<Option<Stato>> = Mutex::new(None);

fn metti(nome: &str, fase: Fase) {
    if let Ok(mut g) = ORA.lock() {
        let da = match g.as_ref() {
            Some(s) if s.nome == nome && s.fase != Fase::Errore => s.da,
            _ => Instant::now(),
        };
        *g = Some(Stato { nome: nome.to_string(), fase, da });
    }
}

pub fn scarica(nome: &str, fatti: u64, totale: u64) {
    metti(nome, Fase::Scarica { fatti, totale });
}

pub fn aggiorna(nome: &str) {
    metti(nome, Fase::Aggiorna);
}

pub fn errore(nome: &str) {
    metti(nome, Fase::Errore);
}

/// Fine del lavoro su `nome`: la riga torna com'era (un errore resta qualche secondo).
pub fn fine(nome: &str) {
    if let Ok(mut g) = ORA.lock() {
        if matches!(g.as_ref(), Some(s) if s.nome == nome && s.fase != Fase::Errore) {
            *g = None;
        }
    }
}

/// Toglie qualunque stato (anche un errore): per le foto della finestra.
pub fn azzera() {
    if let Ok(mut g) = ORA.lock() {
        *g = None;
    }
}

pub fn attuale() -> Option<Stato> {
    let mut g = ORA.lock().ok()?;
    if matches!(g.as_ref(), Some(s) if s.fase == Fase::Errore && s.da.elapsed() > ERRORE_PER) {
        *g = None;
    }
    g.clone()
}

/// Tiene la riga accesa finché vive: si chiude da sola anche se il download esce con `?`.
pub struct Lavoro(String);

impl Lavoro {
    pub fn scarica(nome: &str) -> Self {
        scarica(nome, 0, 0);
        Lavoro(nome.to_string())
    }

    pub fn aggiorna(nome: &str) -> Self {
        aggiorna(nome);
        Lavoro(nome.to_string())
    }

    pub fn avanza(&self, fatti: u64, totale: u64) {
        scarica(&self.0, fatti, totale);
    }

    /// Il download non è riuscito (non vale per «Annullato»).
    pub fn fallito(&self, e: &str) {
        if !e.contains("Annullato") && !e.eq_ignore_ascii_case("annullato") {
            errore(&self.0);
        }
    }
}

impl Drop for Lavoro {
    fn drop(&mut self) {
        fine(&self.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn riga_di_un_componente() {
        {
            let l = Lavoro::scarica("prova-x");
            l.avanza(5, 10);
            assert!(matches!(attuale(), Some(Stato { fase: Fase::Scarica { fatti: 5, totale: 10 }, .. })));
        }
        assert!(attuale().map(|s| s.nome != "prova-x").unwrap_or(true));
        {
            let l = Lavoro::scarica("prova-y");
            l.fallito("errore di rete");
        }
        assert!(matches!(attuale(), Some(Stato { fase: Fase::Errore, ref nome, .. }) if nome == "prova-y"));
        {
            let l = Lavoro::scarica("prova-z");
            l.fallito("Annullato");
        }
        assert!(attuale().map(|s| s.nome != "prova-z").unwrap_or(true));
    }
}
