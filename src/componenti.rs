//! I componenti che Cedro scarica al primo uso (yt-dlp, ffmpeg, deno, motori e modelli):
//! chi li scarica o aggiorna lo dice qui, la riga di stato in alto lo mostra.

use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature="android-worker",derive(serde::Serialize,serde::Deserialize))]
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
type Osservatore = std::sync::Arc<dyn Fn(Option<Stato>) + Send + Sync>;
static OSSERVATORE: Mutex<Option<Osservatore>> = Mutex::new(None);

/// Trasporto facoltativo per l'host Android. Nessun thread di polling nel motore.
/// La callback viene chiamata fuori dal lock dello stato e deve restare breve.
pub fn osserva(callback: Option<Osservatore>) {
    *OSSERVATORE.lock().unwrap_or_else(|e| e.into_inner()) = callback;
}
type Permesso = std::sync::Arc<dyn Fn(&str,u64)->Result<(),String> + Send + Sync>;
static PERMESSO: Mutex<Option<Permesso>> = Mutex::new(None);
/// L'host può verificare rete e spazio prima che inizi un download di componenti.
pub fn verifica_download(callback:Option<Permesso>){*PERMESSO.lock().unwrap_or_else(|e|e.into_inner())=callback;}
pub fn prima_download(nome:&str,bytes:u64)->Result<(),String>{
    let callback=PERMESSO.lock().unwrap_or_else(|e|e.into_inner()).clone();
    if let Some(callback)=callback{callback(nome,bytes)}else{Ok(())}
}
type Parziale = std::sync::Arc<dyn Fn(&std::path::Path)->Result<(),String> + Send + Sync>;
static PARZIALE:Mutex<Option<Parziale>>=Mutex::new(None);
pub fn traccia_parziali(callback:Option<Parziale>){*PARZIALE.lock().unwrap_or_else(|e|e.into_inner())=callback;}
/// Reclama soltanto un file nuovo; quelli preesistenti non appartengono al lavoro corrente.
pub fn prepara_parziale(path:&std::path::Path)->Result<(),String>{
    let callback=PARZIALE.lock().unwrap_or_else(|e|e.into_inner()).clone();
    let Some(callback)=callback else{return Ok(())};
    match std::fs::OpenOptions::new().write(true).create_new(true).open(path){
        Ok(file)=>{drop(file);let result=callback(path);if result.is_err(){let _=std::fs::remove_file(path);}result},
        Err(e) if e.kind()==std::io::ErrorKind::AlreadyExists=>Ok(()),
        Err(e)=>Err(e.to_string()),
    }
}
/// Ricontrolla la policy durante i download lunghi; al massimo una volta al secondo.
/// L'host può attendere il ritorno del Wi-Fi mentre il downloader conserva l'offset.
pub fn continua_download(nome:&str)->Result<(),String>{
    static ULTIMO:Mutex<Option<Instant>>=Mutex::new(None);
    let mut last=ULTIMO.lock().unwrap_or_else(|e|e.into_inner());
    if last.is_some_and(|t|t.elapsed()<Duration::from_secs(1)){return Ok(());}
    *last=Some(Instant::now());drop(last);
    prima_download(nome,0)
}
fn notifica() {
    let callback = OSSERVATORE.lock().unwrap_or_else(|e| e.into_inner()).clone();
    if let Some(callback) = callback { callback(attuale()); }
}

fn metti(nome: &str, fase: Fase) {
    if let Ok(mut g) = ORA.lock() {
        let da = match g.as_ref() {
            Some(s) if s.nome == nome && s.fase != Fase::Errore => s.da,
            _ => Instant::now(),
        };
        *g = Some(Stato { nome: nome.to_string(), fase, da });
    }
    notifica();
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
    notifica();
}

/// Toglie qualunque stato (anche un errore): per le foto della finestra.
pub fn azzera() {
    if let Ok(mut g) = ORA.lock() {
        *g = None;
    }
    notifica();
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
