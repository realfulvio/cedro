//! Journal durevole: identità del lavoro, struttura e transizioni prima degli effetti esterni.
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};

pub type Riga = (PathBuf, String, Option<String>);

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Identita {
    pub chiave: String,
    pub origine: String,
    pub etichetta: String,
    pub conv: bool,
    pub quando: u64,
    pub sorgente: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Forma { File, Directory, Insieme }

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Destinazione {
    pub path: PathBuf,
    pub nome: String,
    /// API26–28: creato prima di Preparato. Assenza del temporaneo + presenza del finale
    /// prova il rename; non confonde un vecchio Replace col nuovo risultato.
    pub temporaneo: Option<PathBuf>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum StatoUscita { Privato, Preparato(Destinazione), Pubblicato(Destinazione) }

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Elemento {
    pub privato: PathBuf,
    pub relativo: PathBuf,
    pub stato: StatoUscita,
    pub errore: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Registro {
    pub versione: u32,
    pub lavoro: Identita,
    pub forma: Forma,
    /// Namespace comune all'insieme: non rinomina il solo header MRXS.
    pub gruppo: Option<String>,
    pub messaggio: String,
    pub pubblico: Option<String>,
    pub elementi: Vec<Elemento>,
}

fn relativo_sicuro(p: &Path) -> bool {
    !p.as_os_str().is_empty() && p.components().all(|c| matches!(c,std::path::Component::Normal(_)))
}

impl Registro {
    pub fn prepara(root: &Path, principale: &Path, lavoro: Identita, messaggio: String) -> Result<Self,String> {
        fn visita(dir: &Path,paths:&mut Vec<PathBuf>)->Result<(),String>{
            for entry in std::fs::read_dir(dir).map_err(|e|e.to_string())? {
                let e=entry.map_err(|e|e.to_string())?;
                if e.file_name().to_string_lossy().starts_with('.') {continue;}
                let t=e.file_type().map_err(|e|e.to_string())?;
                if t.is_dir(){visita(&e.path(),paths)?;}
                else if t.is_file(){paths.push(e.path());}
                else {return Err("Risultato non regolare".into());}
            }
            Ok(())
        }
        let rel=principale.strip_prefix(root).map_err(|_|"Risultato fuori dalla cartella del lavoro")?;
        if !relativo_sicuro(rel){return Err("Risultato fuori dalla cartella del lavoro".into());}
        let mut paths=Vec::new();
        visita(root,&mut paths)?;
        if principale.is_file()&&!paths.iter().any(|p|p==principale){paths.push(principale.into());}
        if paths.is_empty(){return Err("Il lavoro non ha prodotto file".into());}
        paths.sort();paths.dedup();paths.sort_by_key(|p|p!=principale);
        let forma=if principale.is_dir(){Forma::Directory}else if paths.iter().any(|p|p.strip_prefix(root).is_ok_and(|r|r.components().count()>1)){Forma::Insieme}else{Forma::File};
        let gruppo=(forma!=Forma::File).then(||format!("{}-{}",crate::storage::nome_sicuro(principale.file_stem().and_then(|s|s.to_str()).unwrap_or("risultati")),crate::storage::nome_sicuro(&lavoro.chiave)));
        let elementi=paths.into_iter().map(|p|{
            let relativo=p.strip_prefix(root).map_err(|_|"Risultato fuori dalla cartella del lavoro")?.to_path_buf();
            if !relativo_sicuro(&relativo){return Err("Risultato fuori dalla cartella del lavoro");}
            Ok(Elemento{privato:p,relativo,stato:StatoUscita::Privato,errore:None})
        }).collect::<Result<Vec<_>,_>>()?;
        Ok(Self{versione:3,lavoro,forma,gruppo,messaggio,pubblico:None,elementi})
    }

    pub fn salva(&self,root:&Path)->Result<(),String>{
        let data=serde_json::to_vec(self).map_err(|e|e.to_string())?;
        crate::storage::atomico(&root.join(".pronti"),&data).map_err(|e|e.to_string())
    }

    pub fn leggi(root:&Path)->Result<Self,String>{
        let file=std::fs::File::open(root.join(".pronti")).map_err(|e|e.to_string())?;
        let r:Self=serde_json::from_reader(file).map_err(|e|e.to_string())?;
        if r.versione!=3||r.elementi.iter().any(|e|!e.privato.starts_with(root)||!relativo_sicuro(&e.relativo)){
            return Err("Registro dei risultati non valido".into());
        }
        Ok(r)
    }

    pub fn prenota(&mut self,root:&Path,i:usize,d:Destinazione)->Result<(),String>{
        if self.gruppo.is_some(){
            if d.nome.starts_with("Documents/Cedro/"){self.pubblico=Some("Documents/Cedro".into());}
            if d.nome.starts_with("Download/Cedro/"){self.pubblico=Some("Download/Cedro".into());}
        }
        self.elementi[i].stato=StatoUscita::Preparato(d);
        self.salva(root)
    }

    pub fn completato(&mut self,root:&Path,i:usize,d:Destinazione)->Result<(),String>{
        if self.gruppo.is_some(){
            if d.nome.starts_with("Documents/Cedro/"){self.pubblico=Some("Documents/Cedro".into());}
            if d.nome.starts_with("Download/Cedro/"){self.pubblico=Some("Download/Cedro".into());}
        }
        self.elementi[i].stato=StatoUscita::Pubblicato(d);
        self.elementi[i].errore=None;
        self.salva(root)?;
        // Deliberatamente dopo il commit durevole dell'URI.
        let _=std::fs::remove_file(&self.elementi[i].privato);
        Ok(())
    }

    pub fn righe(&self)->Vec<Riga>{
        self.elementi.iter().map(|e|match &e.stato{
            StatoUscita::Pubblicato(d)=>(d.path.clone(),d.nome.clone(),e.errore.clone()),
            _=>(e.privato.clone(),e.relativo.to_string_lossy().into_owned(),Some(e.errore.clone().unwrap_or_else(||"Copia in Cedro".into()))),
        }).collect()
    }

    pub fn sottocartella(&self,i:usize)->PathBuf{
        match &self.gruppo {
            Some(g)=>PathBuf::from(g).join(self.elementi[i].relativo.parent().unwrap_or_else(||Path::new(""))),
            None=>PathBuf::new(),
        }
    }
}

/// Una revisione già persistita ha precedenza sul cleanup, anche se il worker termina dopo save().
pub fn sorgente_revisione(root:&Path)->Option<PathBuf>{
    let source:PathBuf=serde_json::from_reader(std::fs::File::open(root.join(".revisione-source")).ok()?).ok()?;
    (source.starts_with(root.join(".revisione"))&&source.is_file()&&cedro::revisione::has(&source)).then_some(source)
}

pub fn recupera_revisione(root:&Path)->Result<Registro,String>{
    let source=sorgente_revisione(root).ok_or("Nessuna revisione salvata")?;
    let mut lavoro:Identita=serde_json::from_reader(std::fs::File::open(root.join(".lavoro")).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    lavoro.sorgente=Some(source);
    Ok(Registro{versione:3,lavoro,forma:Forma::File,gruppo:None,messaggio:"Interrotto dalla chiusura di Cedro: avvia di nuovo il lavoro".into(),pubblico:None,elementi:Vec::new()})
}

pub fn mediastore_moderno(api:i32)->bool{api>=29}

/// Copia atomica API26–28. Checkpoint dopo creazione temporaneo, prima della copia.
/// Un errore conserva il privato e un marker temporaneo vuoto: il vecchio finale non è il nuovo.
pub fn legacy(privato:&Path,dir:&Path,mode:u8,cancel:&std::sync::atomic::AtomicBool,checkpoint:&mut dyn FnMut(Destinazione)->Result<(),String>)->Result<Destinazione,String>{
    use std::io::{Read,Write};
    use std::sync::atomic::Ordering;
    std::fs::create_dir_all(dir).map_err(|e|e.to_string())?;
    let name=privato.file_name().ok_or("Nome del risultato assente")?;
    let mut dest=dir.join(name);
    if dest.exists(){
        if mode==2{let d=Destinazione{nome:format!("Già presente: {}",dest.display()),path:dest,temporaneo:None};checkpoint(d.clone())?;return Ok(d);}
        if mode!=1 {
            let stem=privato.file_stem().unwrap_or_default().to_string_lossy();
            let ext=privato.extension().map(|v|format!(".{}",v.to_string_lossy())).unwrap_or_default();
            let mut n=2;
            while dest.exists(){dest=dir.join(format!("{stem} ({n}){ext}"));n+=1;}
        }
    }
    let tempdir=crate::storage::nuova_cartella(dir).map_err(|e|e.to_string())?;
    let temp=tempdir.join(".contenuto");
    let mut file=std::fs::OpenOptions::new().write(true).create_new(true).open(&temp).map_err(|e|e.to_string())?;
    let d=Destinazione{nome:dest.to_string_lossy().into_owned(),path:dest.clone(),temporaneo:Some(temp.clone())};
    struct Parziale(PathBuf);
    impl Drop for Parziale{fn drop(&mut self){svuota_temporaneo(&self.0);}}
    let _parziale=Parziale(temp.clone());
    checkpoint(d.clone())?;
    let mut source=std::fs::File::open(privato).map_err(|e|e.to_string())?;
    let mut buf=[0u8;64*1024];
    loop{
        if cancel.load(Ordering::Acquire){return Err("Annullato".into());}
        let n=source.read(&mut buf).map_err(|e|e.to_string())?;if n==0{break;}
        file.write_all(&buf[..n]).map_err(|e|e.to_string())?;
    }
    file.sync_all().map_err(|e|e.to_string())?;
    if cancel.load(Ordering::Acquire){return Err("Annullato".into());}
    std::fs::rename(&temp,&dest).map_err(|e|e.to_string())?;
    std::fs::File::open(dir).and_then(|f|f.sync_all()).map_err(|e|e.to_string())?;
    let _=std::fs::remove_dir(tempdir);
    Ok(d)
}

/// Conserva l'evidenza "non committato" senza trattenere byte di una copia fallita.
pub fn svuota_temporaneo(path:&Path){
    if let Ok(file)=std::fs::OpenOptions::new().write(true).open(path){let _=file.set_len(0);let _=file.sync_all();}
}

pub fn legacy_committed(d:&Destinazione)->bool{
    d.path.is_file()&&d.temporaneo.as_ref().is_none_or(|p|!p.exists())
}

#[cfg(test)]
mod tests{
    use super::*;
    fn identity()->Identita{Identita{chiave:"durable-job".into(),origine:"vetrino.svs".into(),etichetta:"Tessere".into(),conv:false,quando:1,sorgente:None}}
    // Un MIRAX minimo realmente riapribile dal motore, con un JPEG già in fixtures.
    fn mirax(root:&Path)->PathBuf{
        let dir=root.join("slide");std::fs::create_dir_all(&dir).unwrap();
        let jpeg=include_bytes!("../../tests/fixtures/scan.jpg");
        std::fs::write(dir.join("Data0000.dat"),jpeg).unwrap();
        let mut index=b"01.02test-slide".to_vec();
        let table=index.len() as i32+8;
        // Radici, tabella di un livello, testa pagina, record (indice, offset, len, file).
        for n in [table,0,table+4,0,table+12,1,0,0,0,jpeg.len() as i32,0]{index.extend_from_slice(&n.to_le_bytes());}
        std::fs::write(dir.join("Index.dat"),index).unwrap();
        std::fs::write(dir.join("Slidedat.ini"),"[GENERAL]\nSLIDE_ID=test-slide\nIMAGENUMBER_X=1\nIMAGENUMBER_Y=1\n[HIERARCHICAL]\nINDEXFILE=Index.dat\nHIER_COUNT=1\nNONHIER_COUNT=0\nHIER_0_NAME=Slide zoom level\nHIER_0_COUNT=1\nHIER_0_VAL_0_SECTION=L0\n[DATAFILE]\nFILE_COUNT=1\nFILE_0=Data0000.dat\n[L0]\nIMAGE_CONCAT_FACTOR=0\nOVERLAP_X=0\nOVERLAP_Y=0\nMICROMETER_PER_PIXEL_X=0.243\nIMAGE_FILL_COLOR_BGR=16777215\nIMAGE_FORMAT=JPEG\nDIGITIZER_WIDTH=900\nDIGITIZER_HEIGHT=200\n").unwrap();
        let main=root.join("slide.mrxs");std::fs::write(&main,b"").unwrap();main
    }
    #[test]
    fn tiles_e_mrxs_esportati_si_riaprono_con_percorsi_relativi(){
        let root=crate::storage::nuova_cartella(&std::env::temp_dir()).unwrap();
        let main=mirax(&root);
        let before=cedro::mrxs::open(&main).unwrap().read_level(&main,0).unwrap();
        let mut r=Registro::prepara(&root,&main,identity(),"Anonimo".into()).unwrap();r.salva(&root).unwrap();
        assert_eq!(r.forma,Forma::Insieme);assert_eq!(r.elementi.len(),4);
        let exported=crate::storage::nuova_cartella(&std::env::temp_dir()).unwrap();
        for i in 0..r.elementi.len(){
            let src=r.elementi[i].privato.clone();let dir=exported.join(r.sottocartella(i));
            let d=legacy(&src,&dir,1,&std::sync::atomic::AtomicBool::new(false),&mut |d|r.prenota(&root,i,d)).unwrap();
            r.completato(&root,i,d).unwrap();
        }
        let exported_main=exported.join(r.gruppo.as_ref().unwrap()).join("slide.mrxs");
        let after=cedro::mrxs::open(&exported_main).unwrap().read_level(&exported_main,0).unwrap();
        assert_eq!(before.into_raw(),after.into_raw());
        let tiles_root=crate::storage::nuova_cartella(&std::env::temp_dir()).unwrap();
        let tile_dir=tiles_root.join("tiles/0");std::fs::create_dir_all(&tile_dir).unwrap();
        std::fs::write(tile_dir.join("0_0.jpg"),include_bytes!("../../tests/fixtures/scan.jpg")).unwrap();
        let mut tiles=Registro::prepara(&tiles_root,&tiles_root.join("tiles"),identity(),"Tessere".into()).unwrap();
        assert_eq!(tiles.forma,Forma::Directory);assert!(tiles.elementi.iter().all(|e|e.privato.is_file()));
        let d=legacy(&tiles.elementi[0].privato.clone(),&exported.join(tiles.sottocartella(0)),1,&std::sync::atomic::AtomicBool::new(false),&mut |d|tiles.prenota(&tiles_root,0,d)).unwrap();
        assert_eq!(std::fs::read(d.path).unwrap().as_slice(),&include_bytes!("../../tests/fixtures/scan.jpg")[..]);
        for dir in [root,exported,tiles_root]{std::fs::remove_dir_all(dir).unwrap();}
    }
    #[test]
    fn arresti_prima_e_dopo_commit_preservano_identita_e_tutte_le_uscite(){
        let root=crate::storage::nuova_cartella(&std::env::temp_dir()).unwrap();
        for n in ["a.pdf","b.pdf"]{std::fs::write(root.join(n),n).unwrap();}
        let mut r=Registro::prepara(&root,&root.join("a.pdf"),identity(),"Diviso".into()).unwrap();r.salva(&root).unwrap();
        assert!(Registro::leggi(&root).unwrap().elementi.iter().all(|e|e.stato==StatoUscita::Privato));
        let uri=Destinazione{path:"content://media/10".into(),nome:"a.pdf".into(),temporaneo:None};
        r.prenota(&root,0,uri.clone()).unwrap();
        let mut restart=Registro::leggi(&root).unwrap();assert_eq!(restart.lavoro.chiave,"durable-job");
        assert_eq!(restart.elementi[0].stato,StatoUscita::Preparato(uri.clone()));assert!(root.join("a.pdf").exists());
        restart.completato(&root,0,uri.clone()).unwrap();
        let mut restart=Registro::leggi(&root).unwrap();assert!(!root.join("a.pdf").exists());
        assert_eq!(restart.righe()[0].0,uri.path);assert_eq!(restart.righe()[1].0,root.join("b.pdf"));
        let second=Destinazione{path:"content://media/11".into(),nome:"b.pdf".into(),temporaneo:None};
        restart.prenota(&root,1,second.clone()).unwrap();
        let mut restart=Registro::leggi(&root).unwrap();
        // Simula journal Published sincronizzato, ma arresto prima della rimozione privata.
        restart.elementi[1].stato=StatoUscita::Pubblicato(second.clone());restart.salva(&root).unwrap();
        assert!(root.join("b.pdf").exists());
        let mut restart=Registro::leggi(&root).unwrap();restart.completato(&root,1,second).unwrap();
        assert!(!root.join("b.pdf").exists());
        assert_eq!(Registro::leggi(&root).unwrap().righe().iter().filter(|u|u.0.to_string_lossy().starts_with("content://")).count(),2);
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn legacy_replace_e_cancel_non_confondono_vecchio_e_nuovo(){
        let root=crate::storage::nuova_cartella(&std::env::temp_dir()).unwrap();
        let public=root.join("public");std::fs::create_dir_all(&public).unwrap();
        let private=root.join("x.pdf");std::fs::write(&private,b"new").unwrap();std::fs::write(public.join("x.pdf"),b"old").unwrap();
        let mut prepared=None;
        assert!(legacy(&private,&public,1,&std::sync::atomic::AtomicBool::new(true),&mut |d|{prepared=Some(d);Ok(())}).is_err());
        assert!(!legacy_committed(prepared.as_ref().unwrap()));assert_eq!(std::fs::read(public.join("x.pdf")).unwrap(),b"old");
        let done=legacy(&private,&public,1,&std::sync::atomic::AtomicBool::new(false),&mut |_|Ok(())).unwrap();
        assert!(legacy_committed(&done));assert_eq!(std::fs::read(done.path).unwrap(),b"new");assert!(private.exists());
        for api in [26,27,28]{assert!(!mediastore_moderno(api));}for api in [29,30,34]{assert!(mediastore_moderno(api));}
        std::fs::remove_dir_all(root).unwrap();
    }
}
