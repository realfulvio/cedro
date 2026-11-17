//! Contesto persistente del selettore esterno: Android può ricreare il processo prima del risultato.
use std::io;
use std::path::{Path,PathBuf};
use serde::{Deserialize,Serialize};
use crate::ui::Sorgente;

#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
pub(crate) enum Quando { Oggi, Firma, Giorno(i64) }
impl Quando {
    fn da(at:cedro::firma::At)->Self {
        match at {cedro::firma::At::Now=>Self::Oggi,cedro::firma::At::Signing=>Self::Firma,cedro::firma::At::Custom(t)=>Self::Giorno(t)}
    }
    pub(crate) fn at(&self)->cedro::firma::At {
        match *self {Self::Oggi=>cedro::firma::At::Now,Self::Firma=>cedro::firma::At::Signing,Self::Giorno(t)=>cedro::firma::At::Custom(t)}
    }
}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
pub(crate) struct Originale {pub path:PathBuf,pub quando:Quando}
#[derive(Clone,Debug,PartialEq,Eq,Serialize,Deserialize)]
pub(crate) struct Attesa {
    pub sorgente:Sorgente,
    pub originale:Option<Originale>,
}
impl Attesa {
    pub(crate) fn nuova(sorgente:Sorgente,viewer:Option<(&Path,cedro::firma::At)>)->io::Result<Self> {
        let originale=if sorgente==Sorgente::Originale {
            let (path,at)=viewer.ok_or_else(||io::Error::other("Visualizzatore firma assente"))?;
            Some(Originale{path:path.to_path_buf(),quando:Quando::da(at)})
        }else{None};
        Ok(Self{sorgente,originale})
    }
    /// Prima di avviare DocumentsUI: rename+fsync come gli altri journal Android.
    pub(crate) fn salva(&self,path:&Path)->io::Result<()> {
        crate::storage::atomico(path,&serde_json::to_vec(&Some(self))?)
    }
    pub(crate) fn leggi(path:&Path)->io::Result<Option<Self>> {
        let bytes=match std::fs::read(path) {Ok(v)=>v,Err(e) if e.kind()==io::ErrorKind::NotFound=>return Ok(None),Err(e)=>return Err(e)};
        let pending:Option<Self>=serde_json::from_slice(&bytes)?;
        if pending.as_ref().is_some_and(|p|(p.sorgente==Sorgente::Originale)!=p.originale.is_some()) {
            return Err(io::Error::other("Contesto selettore incompleto"));
        }
        Ok(pending)
    }
    /// Tombstone atomica: un risultato consumato o annullato non viene riproposto al riavvio.
    pub(crate) fn chiudi(path:&Path)->io::Result<()> {crate::storage::atomico(path,b"null")}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn picker_context_survives_recreation_and_clears_without_removing_inputs() {
        let dir=crate::storage::nuova_cartella(&std::env::temp_dir()).unwrap();
        let journal=dir.join("scelta.json");let signature=dir.join("firma.p7s");
        std::fs::write(&signature,b"signature fixture").unwrap();
        assert_eq!(Attesa::leggi(&journal).unwrap(),None);
        for source in [Sorgente::Foto,Sorgente::Documenti,Sorgente::Audio,Sorgente::Altri,Sorgente::PdfAltri,Sorgente::Coda,Sorgente::Cartella,Sorgente::Firmati,Sorgente::Originale] {
            for at in [cedro::firma::At::Now,cedro::firma::At::Signing,cedro::firma::At::Custom(1700000000)] {
                let expected=Attesa::nuova(source,Some((&signature,at))).unwrap();
                expected.salva(&journal).unwrap();
                // Nuova lettura da disco, senza stato in memoria del vecchio processo.
                let recovered=Attesa::leggi(&journal).unwrap().unwrap();assert_eq!(recovered,expected);
                if source==Sorgente::Originale {
                    let original=recovered.originale.unwrap();assert_eq!(original.path,signature);assert_eq!(original.quando.at(),at);
                }else{assert!(recovered.originale.is_none());}
                Attesa::chiudi(&journal).unwrap();
                assert_eq!(Attesa::leggi(&journal).unwrap(),None);
                assert_eq!(Attesa::leggi(&journal).unwrap(),None,"nessun replay dopo secondo riavvio");
                assert_eq!(std::fs::read(&signature).unwrap(),b"signature fixture");
            }
        }
        assert!(Attesa::nuova(Sorgente::Originale,None).is_err());
        std::fs::write(&journal,br#"{"sorgente":"Originale","originale":null}"#).unwrap();
        assert!(Attesa::leggi(&journal).is_err());
        Attesa::chiudi(&journal).unwrap();assert_eq!(Attesa::leggi(&journal).unwrap(),None);
        std::fs::remove_dir_all(dir).unwrap();
    }
}
