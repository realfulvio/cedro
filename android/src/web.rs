//! Scelte del pannello web: snapshot separato da qualità di conversione e Voce.
use serde::{Deserialize,Serialize};
use std::path::{Path,PathBuf};

#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]
#[serde(default)]
pub struct Scelte {
    pub altezza:u32, pub audio_quality:u32,
    pub video_format:String, pub audio_format:String,
    pub subtitles:u32, pub thumbnail:bool, pub chapters:bool, pub metadata:bool,
    pub split_chapters:bool, pub skip_sponsors:bool, pub normalize:bool,
    pub playlist:bool, pub skip_existing:bool,
    pub media_quality:u8, pub voce:crate::worker::Voce,
}
impl Default for Scelte {
    fn default()->Self{Self{altezza:1080,audio_quality:1,video_format:"mp4".into(),audio_format:"mp3".into(),subtitles:0,thumbnail:true,chapters:true,metadata:true,split_chapters:false,skip_sponsors:false,normalize:false,playlist:false,skip_existing:true,media_quality:82,voce:Default::default()}}
}
impl Scelte {
    pub fn richiesta(&self,url:&str,azione:&str,dest:PathBuf)->cedro::youtube::Request {
        let audio=matches!(azione,"web-audio"|"web-testo");
        let mut r=cedro::youtube::Request::new(vec![url.into()],audio,dest);
        r.quality=if audio{self.audio_quality}else{self.altezza};
        r.format=if audio{self.audio_format.clone()}else{self.video_format.clone()};
        r.subtitles=self.subtitles;r.thumbnail=self.thumbnail;r.chapters=self.chapters;r.metadata=self.metadata;
        r.split_chapters=self.split_chapters;r.skip_sponsors=self.skip_sponsors;r.normalize=self.normalize;
        r.playlist=self.playlist;r.skip_existing=self.skip_existing;
        r
    }
}

/// Applica il modello del core una sola volta, preservando le sottocartelle della raccolta.
pub fn nomina(file:&Path,dir:&Path,opts:&cedro::Options)->Result<PathBuf,String>{
    std::fs::create_dir_all(dir).map_err(|e|e.to_string())?;
    let stem=file.file_stem().and_then(|s|s.to_str()).ok_or("Nome del risultato assente")?;
    let ext=file.extension().and_then(|s|s.to_str()).unwrap_or("");
    let target=cedro::choose_output(file,dir,stem,ext,opts).ok_or("Il file esiste già: saltato")?;
    if file!=target{std::fs::rename(file,&target).map_err(|e|e.to_string())?;}
    Ok(target)
}

/// Stato serializzabile: l'orologio monotono viene creato dal ricevente, mai passato fra processi.
#[derive(Clone,Debug,PartialEq,Serialize,Deserialize)]
pub struct Componente {pub nome:String,pub fase:cedro::componenti::Fase}
impl Componente {
    pub fn da_core(s:cedro::componenti::Stato)->Self{Self{nome:s.nome,fase:s.fase}}
    pub fn testo(&self)->String{
        use cedro::componenti::Fase;
        let nome=cedro::i18n::tr(&self.nome);
        match self.fase {
            Fase::Scarica{fatti,totale}=>if totale>0 {format!("{nome} · {}% · {} / {}",fatti.saturating_mul(100)/totale,cedro::size_words(fatti),cedro::size_words(totale))}else{format!("{nome} · {}",cedro::size_words(fatti))},
            Fase::Aggiorna=>format!("{} · {nome}",cedro::i18n::tr("Aggiornamento")),
            Fase::Errore=>format!("{nome} · {}",cedro::i18n::tr("Download non riuscito. Riprova.")),
        }
    }
    pub fn percento(&self)->i32 {match self.fase{cedro::componenti::Fase::Scarica{fatti,totale} if totale>0=>(fatti.saturating_mul(100)/totale).min(100) as i32,_=>-1}}
}

#[derive(Default)]
pub struct Componenti(std::collections::BTreeMap<u32,(Componente,std::time::Instant)>);
impl Componenti {
    pub fn ricevi(&mut self,id:u32,s:Option<Componente>){
        if let Some(s)=s {if self.0.get(&id).is_none_or(|(old,_)|*old!=s){self.0.insert(id,(s,std::time::Instant::now()));}}
        else if self.0.get(&id).is_some_and(|(s,_)|s.fase!=cedro::componenti::Fase::Errore){self.0.remove(&id);}
    }
    pub fn finisci(&mut self,id:u32,annullato:bool){if annullato{self.0.remove(&id);}else{self.ricevi(id,None);}}
    pub fn attuale(&mut self)->Option<Componente>{self.0.retain(|_,(s,t)|s.fase!=cedro::componenti::Fase::Errore||t.elapsed()<std::time::Duration::from_secs(12));self.0.values().next_back().map(|(s,_)|s.clone())}
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scelte_nel_request_e_voce_separata(){
        let s=Scelte{altezza:480,audio_quality:2,video_format:"mkv".into(),audio_format:"flac".into(),subtitles:3,thumbnail:false,chapters:false,metadata:false,split_chapters:true,skip_sponsors:true,normalize:true,playlist:true,skip_existing:false,media_quality:91,voce:crate::worker::Voce{modello:2,voci:4}};
        let s:Scelte=serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        for action in ["web-video","web-audio","web-testo"] {
            let r=s.richiesta("https://youtu.be/test",action,"out".into());
            assert_eq!(r.quality,if r.audio{2}else{480});assert_eq!(r.format,if r.audio{"flac"}else{"mkv"});
            assert_eq!(r.subtitles,3);assert!(!r.thumbnail&&!r.chapters&&!r.metadata);
            assert!(r.split_chapters&&r.skip_sponsors&&r.normalize&&r.playlist&&!r.skip_existing);
        }
        assert_eq!(s.media_quality,91);assert_eq!(s.voce.voci,4);
    }
    #[test]
    fn stato_ipc_errore_fine_cancel_e_scadenza(){
        let e=crate::worker::Evento::Componente(Some(Componente{nome:"Python".into(),fase:cedro::componenti::Fase::Scarica{fatti:50,totale:100}}));
        let crate::worker::Evento::Componente(s)=serde_json::from_slice(&serde_json::to_vec(&e).unwrap()).unwrap() else{panic!()};
        let mut c=Componenti::default();c.ricevi(1,s);assert_eq!(c.attuale().unwrap().percento(),50);
        c.finisci(1,false);assert!(c.attuale().is_none());
        c.ricevi(1,Some(Componente{nome:"Python".into(),fase:cedro::componenti::Fase::Errore}));c.finisci(1,false);assert!(c.attuale().is_some());
        c.0.get_mut(&1).unwrap().1-=std::time::Duration::from_secs(13);assert!(c.attuale().is_none());
        c.ricevi(2,Some(Componente{nome:"ffmpeg".into(),fase:cedro::componenti::Fase::Aggiorna}));c.finisci(2,true);assert!(c.attuale().is_none());
    }
    #[test]
    fn nome_caso_su_download_e_sottocartelle(){
        let dir=crate::storage::nuova_cartella(&std::env::temp_dir()).unwrap();
        let file=dir.join("video.mp4");std::fs::write(&file,b"media").unwrap();
        let opts=cedro::caso::opzioni(Some("C-42"),cedro::Options{name_template:Some("%caso%_%nome%".into()),..Default::default()});
        let target=nomina(&file,&dir.join("Raccolta"),&opts).unwrap();
        assert_eq!(target.file_name().unwrap(),"C-42_video.mp4");assert_eq!(std::fs::read(target).unwrap(),b"media");
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[derive(Clone,Debug,PartialEq)]
pub struct AttesaDownload {pub nome:String,pub bytes:u64,pub liberi:u64,pub mobile:bool,pub conferma_spazio:bool}

/// L'ID UI non attraversa un riavvio: ogni figlio usa la chiave durevole batch/indice.
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Figlio {
    pub identita:crate::publication::Identita,
    pub origine:PathBuf,
    pub out:PathBuf,
    pub source:PathBuf,
    pub fine:bool,
}
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Batch {
    pub root:PathBuf,
    pub figli:Vec<Figlio>,
    pub opts:cedro::Options,
    pub voce:crate::worker::Voce,
}
impl Batch {
    pub fn prepara(root:&Path,files:&[PathBuf],identita:&crate::publication::Identita,opts:cedro::Options,voce:crate::worker::Voce)->Result<Self,String>{
        let figli=files.iter().enumerate().map(|(i,file)|{
            let out=root.join(".figli").join(i.to_string());
            let source=out.join(".revisione").join(crate::storage::nome_sicuro(file.file_name().and_then(|n|n.to_str()).unwrap_or("audio")));
            let mut identity=identita.clone();identity.chiave=format!("{}/audio-{i}",identita.chiave);identity.origine=file.file_name().unwrap_or_default().to_string_lossy().into_owned();identity.etichetta="Testo".into();identity.sorgente=Some(source.clone());
            Figlio{identita:identity,origine:file.clone(),out,source,fine:false}
        }).collect();
        let batch=Self{root:root.into(),figli,opts,voce};
        // Prima il piano: anche un crash durante il primo rename è recuperabile.
        batch.salva()?;
        batch.materializza()?;
        Ok(batch)
    }
    pub fn salva(&self)->Result<(),String>{crate::storage::atomico(&self.root.join(".batch"),&serde_json::to_vec(self).map_err(|e|e.to_string())?).map_err(|e|e.to_string())}
    pub fn leggi(root:&Path)->Result<Self,String>{
        let b:Self=serde_json::from_reader(std::fs::File::open(root.join(".batch")).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        if b.root!=root||b.figli.iter().any(|f|!f.out.starts_with(root.join(".figli"))||!f.source.starts_with(f.out.join(".revisione"))||!f.origine.starts_with(root)){return Err("Raccolta non valida".into());}
        Ok(b)
    }
    pub fn materializza(&self)->Result<(),String>{
        for f in &self.figli {
            std::fs::create_dir_all(f.out.join(".revisione")).map_err(|e|e.to_string())?;
            crate::storage::atomico(&f.out.join(".lavoro"),&serde_json::to_vec(&f.identita).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
            crate::storage::atomico(&f.out.join(".batch-child"),f.identita.chiave.as_bytes()).map_err(|e|e.to_string())?;
            if !f.source.is_file(){std::fs::rename(&f.origine,&f.source).map_err(|e|e.to_string())?;}
            crate::storage::atomico(&f.out.join(".revisione-source"),&serde_json::to_vec(&f.source).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        }Ok(())
    }
    pub fn finisci(&mut self,index:usize)->Result<(),String>{self.figli[index].fine=true;self.salva()}
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    #[test]
    fn due_audio_manifest_crash_e_isolamento(){
        let root=crate::storage::nuova_cartella(&std::env::temp_dir()).unwrap();
        let a=root.join("a.wav");let b=root.join("b.wav");std::fs::write(&a,b"primo").unwrap();std::fs::write(&b,b"secondo").unwrap();
        let identity=crate::publication::Identita{chiave:"batch-test".into(),origine:"raccolta".into(),etichetta:"Testo".into(),conv:false,quando:1,sorgente:None,caso:"C1".into()};
        let mut batch=Batch::prepara(&root,&[a,b],&identity,cedro::caso::opzioni(Some("C1"),Default::default()),Default::default()).unwrap();
        let mut recovered=Batch::leggi(&root).unwrap();recovered.materializza().unwrap();
        assert_ne!(recovered.figli[0].identita.chiave,recovered.figli[1].identita.chiave);
        assert_eq!(std::fs::read(&recovered.figli[0].source).unwrap(),b"primo");
        // Un annullamento/fine figlio non pulisce le fonti, né influenza l'altro.
        recovered.finisci(0).unwrap();assert!(!recovered.figli[1].fine);
        assert_eq!(std::fs::read(&recovered.figli[1].source).unwrap(),b"secondo");
        batch=Batch::leggi(&root).unwrap();assert!(batch.figli[0].fine);assert_eq!(batch.figli[1].identita.caso,"C1");
        std::fs::remove_dir_all(root).unwrap();
    }
}
