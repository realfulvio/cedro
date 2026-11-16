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
pub struct AttesaDownload {pub nome:String,pub bytes:u64,pub liberi:u64,pub mobile:bool,pub da:std::time::Instant}

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
    /// Ricevute recuperate, solo nel completamento in memoria: precedono i figli nello stesso evento.
    #[serde(skip)]
    pub ricevute:Vec<crate::publication::Registro>,
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
        let batch=Self{ricevute:Vec::new(),root:root.into(),figli,opts,voce};
        // Prima il piano: anche un crash durante il primo rename è recuperabile.
        batch.salva()?;
        batch.materializza()?;
        Ok(batch)
    }
    pub fn salva(&self)->Result<(),String>{crate::storage::atomico(&self.root.join(".batch"),&serde_json::to_vec(self).map_err(|e|e.to_string())?).map_err(|e|e.to_string())}
    pub fn leggi(root:&Path)->Result<Self,String>{
        let b:Self=serde_json::from_reader(std::fs::File::open(root.join(".batch")).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        if b.root!=root||b.figli.iter().any(|f|!f.out.starts_with(root.join(".figli"))||!f.source.starts_with(f.out.join(".revisione"))||!f.origine.starts_with(root)||[&f.out,&f.source,&f.origine].iter().any(|p|p.components().any(|c|matches!(c,std::path::Component::ParentDir)))){return Err("Raccolta non valida".into());}
        Ok(b)
    }
    pub fn materializza(&self)->Result<(),String>{
        for (index,f) in self.figli.iter().enumerate() {
            std::fs::create_dir_all(f.out.join(".revisione")).map_err(|e|e.to_string())?;
            if let Some(mut saved)=Salvataggio::leggi(&self.root){saved.chiave.push_str(&format!("/audio-{index}"));saved.salva(&f.out)?;}
            crate::storage::atomico(&f.out.join(".lavoro"),&serde_json::to_vec(&f.identita).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
            crate::storage::atomico(&f.out.join(".batch-child"),f.identita.chiave.as_bytes()).map_err(|e|e.to_string())?;
            if !f.source.is_file(){std::fs::rename(&f.origine,&f.source).map_err(|e|e.to_string())?;}
            crate::storage::atomico(&f.out.join(".revisione-source"),&serde_json::to_vec(&f.source).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
        }Ok(())
    }
    /// I figli sono noti dal manifest: non dipendono dal mtime della scansione generale.
    /// Il callback riconcilia anche i commit del provider interrotti; tutto avviene nel worker.
    pub fn recupera_ricevute(&mut self,mut leggi:impl FnMut(&Path)->Option<crate::publication::Registro>){
        self.ricevute=self.figli.iter().filter_map(|f|leggi(&f.out).filter(|r|r.lavoro.chiave==f.identita.chiave)).collect();
    }
    pub fn finisci(&mut self,index:usize)->Result<(),String>{self.figli[index].fine=true;self.salva()}
}

#[cfg(test)]
mod batch_tests {
    use super::*;
    #[test]
    fn recupero_del_piano_prima_del_rename_e_del_marker(){
        for dopo_primo in [false,true]{
            let root=crate::storage::nuova_cartella(&std::env::temp_dir()).unwrap();
            let files=[root.join("a.wav"),root.join("b.wav")];
            for (i,p) in files.iter().enumerate(){std::fs::write(p,[i as u8;4]).unwrap();}
            let identity=crate::publication::Identita{chiave:"batch-crash".into(),origine:"raccolta".into(),etichetta:"Testo".into(),conv:false,quando:1,sorgente:None,caso:"C".into()};
            let batch=Batch::prepara(&root,&files,&identity,Default::default(),Default::default()).unwrap();
            // Fixture del crash: manifest già committato, zero o un rename completato.
            for (i,child) in batch.figli.iter().enumerate(){
                if i==0&&dopo_primo{std::fs::remove_file(child.out.join(".revisione-source")).unwrap();}
                else{std::fs::rename(&child.source,&child.origine).unwrap();std::fs::remove_dir_all(&child.out).unwrap();}
            }
            let recovered=Batch::leggi(&root).unwrap();recovered.materializza().unwrap();recovered.materializza().unwrap();
            for (i,child) in recovered.figli.iter().enumerate(){
                assert_eq!(std::fs::read(&child.source).unwrap(),[i as u8;4]);
                let marker:PathBuf=serde_json::from_reader(std::fs::File::open(child.out.join(".revisione-source")).unwrap()).unwrap();assert_eq!(marker,child.source);
                assert!(child.out.join(".lavoro").is_file());
            }
            std::fs::remove_dir_all(root).unwrap();
        }
    }
    #[test]
    fn due_audio_manifest_crash_e_isolamento(){
        let root=crate::storage::nuova_cartella(&std::env::temp_dir()).unwrap();
        let a=root.join("a.wav");let b=root.join("b.wav");std::fs::write(&a,b"primo").unwrap();std::fs::write(&b,b"secondo").unwrap();
        let identity=crate::publication::Identita{chiave:"batch-test".into(),origine:"raccolta".into(),etichetta:"Testo".into(),conv:false,quando:1,sorgente:None,caso:"C1".into()};
        let _batch=Batch::prepara(&root,&[a,b],&identity,cedro::caso::opzioni(Some("C1"),Default::default()),Default::default()).unwrap();
        let mut recovered=Batch::leggi(&root).unwrap();recovered.materializza().unwrap();
        assert_ne!(recovered.figli[0].identita.chiave,recovered.figli[1].identita.chiave);
        assert_eq!(std::fs::read(&recovered.figli[0].source).unwrap(),b"primo");
        // Un annullamento/fine figlio non pulisce le fonti, né influenza l'altro.
        recovered.finisci(0).unwrap();assert!(!recovered.figli[1].fine);
        assert_eq!(std::fs::read(&recovered.figli[1].source).unwrap(),b"secondo");
        let batch=Batch::leggi(&root).unwrap();assert!(batch.figli[0].fine);assert_eq!(batch.figli[1].identita.caso,"C1");
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// Comprende sidecar e capitoli che yt-dlp può non elencare in after_move.
pub fn uscite_download(root:&Path)->Result<Vec<PathBuf>,String>{
    fn visita(dir:&Path,out:&mut Vec<PathBuf>)->Result<(),String>{
        for entry in std::fs::read_dir(dir).map_err(|e|e.to_string())?{
            let e=entry.map_err(|e|e.to_string())?;let name=e.file_name();
            if name.to_string_lossy().starts_with('.')||e.path().extension().is_some_and(|e|e=="part"){continue;}
            let ty=e.file_type().map_err(|e|e.to_string())?;
            if ty.is_dir(){visita(&e.path(),out)?;}else if ty.is_file(){out.push(e.path());}else{return Err("Risultato non regolare".into());}
        }Ok(())
    }
    let mut out=Vec::new();visita(root,&mut out)?;out.sort();Ok(out)
}

/// Un parziale reclamato con create_new; l'identità del file evita di cancellare un sostituto.
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Parziale {pub path:PathBuf,pub device:u64,pub inode:u64}
impl Parziale {
    #[cfg(unix)]
    pub fn registra(path:&Path)->Result<Self,String>{
        use std::os::unix::fs::MetadataExt;
        let m=std::fs::symlink_metadata(path).map_err(|e|e.to_string())?;
        if !m.is_file(){return Err("Percorso del componente non valido".into());}
        Ok(Self{path:path.into(),device:m.dev(),inode:m.ino()})
    }
}
/// Elenco ricevuto dal worker dopo create_new: nessuna scansione per differenza della cartella.
#[cfg(unix)]
pub fn rimuovi_parziali(registrati:&[Parziale]){
    for file in registrati{if Parziale::registra(&file.path).is_ok_and(|p|p.device==file.device&&p.inode==file.inode){let _=std::fs::remove_file(&file.path);}}
}

pub fn rete_consentita(policy:u8,mobile:bool)->bool{!mobile||policy==2}
pub fn spazio_sufficiente(bytes:u64,liberi:u64)->bool{bytes==0||liberi>=bytes.saturating_add(64_000_000)}

#[cfg(test)]
mod policy_tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn policy_rete_spazio_e_cancel_preserva_preesistenti(){
        for policy in 0..=2{assert!(rete_consentita(policy,false));assert_eq!(rete_consentita(policy,true),policy==2);}
        assert!(!spazio_sufficiente(264_000_000,300_000_000));assert!(spazio_sufficiente(264_000_000,400_000_000));
        assert!(!spazio_sufficiente(u64::MAX,u64::MAX-1));
        let root=crate::storage::nuova_cartella(&std::env::temp_dir()).unwrap();
        let old=root.join("vecchio.part");std::fs::write(&old,b"prima").unwrap();
        let new=root.join("nuovo.part");std::fs::write(&new,b"nuovo").unwrap();
        let model=root.join("modello.bin");std::fs::write(&model,b"pronto").unwrap();
        let altro=root.join("altro-job.part");std::fs::write(&altro,b"altro lavoro").unwrap();
        let owned=Parziale::registra(&new).unwrap();
        let sostituito=root.join("sostituito.part");std::fs::write(&sostituito,b"mio").unwrap();let replaced=Parziale::registra(&sostituito).unwrap();
        std::fs::rename(&altro,&sostituito).unwrap();
        rimuovi_parziali(&[owned,replaced]);assert!(!new.exists());assert!(old.exists());assert!(model.exists());
        assert_eq!(std::fs::read(&sostituito).unwrap(),b"altro lavoro");
        std::fs::remove_dir_all(root).unwrap();
    }
}

/// Firma privata della richiesta, separata dai nomi pubblici. Non contiene la nota del caso.
#[derive(Clone,Debug,Serialize,Deserialize)]
pub struct Salvataggio {pub chiave:String,pub mode:u8,pub salta:bool}
impl Salvataggio {
    pub fn nuovo(url:&str,scelte:&Scelte,opts:&cedro::Options)->Self{
        let mut content=scelte.clone();content.skip_existing=false;
        let chiave=serde_json::json!([url,content,opts.name_template,opts.caso]).to_string();
        // Il toggle riusa solo ricevute compatibili; le collisioni restano una scelta distinta.
        let mode=match opts.exists{cedro::Exists::Replace=>1,cedro::Exists::Skip=>2,_=>0};
        Self{chiave,mode,salta:scelte.skip_existing}
    }
    pub fn salva(&self,root:&Path)->Result<(),String>{crate::storage::atomico(&root.join(".web-request"),&serde_json::to_vec(self).map_err(|e|e.to_string())?).map_err(|e|e.to_string())}
    pub fn leggi(root:&Path)->Option<Self>{serde_json::from_reader(std::fs::File::open(root.join(".web-request")).ok()?).ok()}
}

/// I journal completati sono l'indice: non segna come scaricato un file che può essere sparito.
/// `esiste` verifica il provider reale (MediaStore nel genitore Android, filesystem nei test).
pub fn riusi(base:&Path,root:&Path,r:&mut crate::publication::Registro,esiste:impl Fn(&mut crate::publication::Destinazione)->bool)->Vec<(usize,crate::publication::Destinazione)>{
    let Some(request)=Salvataggio::leggi(root).filter(|s|s.salta) else{return Vec::new()};
    fn cerca(base:&Path,exclude:&Path,key:&str,out:&mut Vec<crate::publication::Registro>){
        if base==exclude{return;}
        if Salvataggio::leggi(base).is_some_and(|s|s.chiave==key){if let Ok(r)=crate::publication::Registro::leggi(base){out.push(r);}}
        if let Ok(entries)=std::fs::read_dir(base){for e in entries.flatten(){if e.file_type().is_ok_and(|t|t.is_dir())&&!matches!(e.file_name().to_str(),Some(".revisione"|".scaricato")){cerca(&e.path(),exclude,key,out);}}}
    }
    let mut old=Vec::new();cerca(base,root,&request.chiave,&mut old);
    old.sort_by(|a,b|(b.lavoro.quando,&b.lavoro.chiave).cmp(&(a.lavoro.quando,&a.lavoro.chiave)));
    let mut relativi:Vec<_>=r.elementi.iter().map(|e|e.relativo.clone()).collect();relativi.sort();
    if let Some(previous)=old.iter().find(|p|{
        let mut names:Vec<_>=p.elementi.iter().map(|e|e.relativo.clone()).collect();names.sort();p.forma==r.forma&&names==relativi
    }){r.gruppo=previous.gruppo.clone();}
    let mut reuse=Vec::new();
    for (i,element) in r.elementi.iter().enumerate(){
        for previous in &old {
            if previous.gruppo!=r.gruppo{continue;}
            let candidate=previous.elementi.iter().find(|e|e.relativo==element.relativo).and_then(|e|match &e.stato{crate::publication::StatoUscita::Pubblicato(d)=>Some(d.clone()),_=>None});
            if let Some(mut d)=candidate{if esiste(&mut d){reuse.push((i,d));break;}}
        }
    }reuse
}

#[cfg(test)]
mod salvataggio_tests {
    use super::*;
    use crate::publication::{Identita,Registro};
    fn job(base:&Path,public:&Path,n:u64,skip:bool)->Registro{
        let root=base.join(n.to_string());std::fs::create_dir_all(&root).unwrap();
        for rel in ["Playlist/01.mp4","Playlist/Capitoli/01.mp4","Playlist/01.it.srt"]{
            let path=root.join(rel);std::fs::create_dir_all(path.parent().unwrap()).unwrap();std::fs::write(path,b"contenuto").unwrap();
        }
        let opts=cedro::caso::opzioni(Some("C-1"),Default::default());
        let choices=Scelte{playlist:true,split_chapters:true,subtitles:1,skip_existing:skip,..Default::default()};
        Salvataggio::nuovo("https://youtu.be/prova",&choices,&opts).salva(&root).unwrap();
        let identity=Identita{chiave:format!("job-{n}"),origine:"raccolta".into(),etichetta:"Video".into(),conv:false,quando:n,sorgente:None,caso:"C-1".into()};
        let mut r=Registro::prepara(&root,&root.join("Playlist/01.mp4"),identity,"ok".into()).unwrap();
        let found=riusi(base,&root,&mut r,|d|d.path.is_file());r.salva(&root).unwrap();
        for i in 0..r.elementi.len(){
            let dest=if let Some((_,d))=found.iter().find(|(index,_)|*index==i){r.prenota(&root,i,d.clone()).unwrap();d.clone()}
            else{
                let file=r.elementi[i].privato.clone();let dir=public.join(r.sottocartella(i));
                crate::publication::legacy(&file,&dir,1,&std::sync::atomic::AtomicBool::new(false),&mut|d|r.prenota(&root,i,d)).unwrap()
            };
            r.completato(&root,i,dest).unwrap();
        }r
    }
    #[test]
    fn richieste_diverse_con_stesso_nome_pubblicano_i_propri_byte(){
        let root=crate::storage::nuova_cartella(&std::env::temp_dir()).unwrap();
        let base=root.join("privati");let public=root.join("pubblici");std::fs::create_dir(&base).unwrap();
        let mut paths=Vec::new();
        for (n,url,height,bytes) in [(0,"https://youtu.be/a",480,b"video a 480".as_slice()),(1,"https://youtu.be/a",1080,b"video a 1080".as_slice()),(2,"https://youtu.be/b",1080,b"video b 1080".as_slice())]{
            let dir=base.join(n.to_string());std::fs::create_dir(&dir).unwrap();
            let file=dir.join("stesso-titolo.mp4");std::fs::write(&file,bytes).unwrap();
            let choices=Scelte{altezza:height,skip_existing:n>0,..Default::default()};
            let saved=Salvataggio::nuovo(url,&choices,&Default::default());saved.salva(&dir).unwrap();
            let identity=Identita{chiave:format!("file-{n}"),origine:"video".into(),etichetta:"Video".into(),conv:false,quando:n,sorgente:None,caso:String::new()};
            let mut receipt=Registro::prepara(&dir,&file,identity,"ok".into()).unwrap();
            assert!(riusi(&base,&dir,&mut receipt,|d|d.path.is_file()).is_empty());
            receipt.salva(&dir).unwrap();
            let dest=crate::publication::legacy(&file,&public,saved.mode,&std::sync::atomic::AtomicBool::new(false),&mut |d|receipt.prenota(&dir,0,d)).unwrap();
            receipt.completato(&dir,0,dest.clone()).unwrap();
            assert_eq!(std::fs::read(&dest.path).unwrap(),bytes);
            assert!(!paths.contains(&dest.path));paths.push(dest.path);
        }
        assert_eq!(std::fs::read(&paths[0]).unwrap(),b"video a 480");
        assert_eq!(std::fs::read(&paths[1]).unwrap(),b"video a 1080");
        assert_eq!(uscite_download(&public).unwrap().len(),3);
        for (exists,mode) in [(cedro::Exists::Replace,1),(cedro::Exists::Skip,2)]{
            assert_eq!(Salvataggio::nuovo("link",&Default::default(),&cedro::Options{exists,..Default::default()}).mode,mode);
        }
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn due_job_riusano_file_gruppo_sidecar_e_ricreano_quello_eliminato(){
        let root=crate::storage::nuova_cartella(&std::env::temp_dir()).unwrap();let base=root.join("privati");let public=root.join("pubblici");std::fs::create_dir_all(&base).unwrap();
        let first=job(&base,&public,1,false);let second=job(&base,&public,2,true);
        assert_eq!(first.gruppo,second.gruppo);
        let paths=|r:&Registro|r.righe().into_iter().map(|v|v.0).collect::<Vec<_>>();
        assert_eq!(paths(&first),paths(&second));assert_eq!(uscite_download(&public).unwrap().len(),3);
        let removed=paths(&first)[1].clone();std::fs::remove_file(&removed).unwrap();
        let third=job(&base,&public,3,true);assert_eq!(third.gruppo,first.gruppo);assert_eq!(paths(&first),paths(&third));assert!(removed.is_file());assert_eq!(uscite_download(&public).unwrap().len(),3);
        // Un URI pending o non più leggibile non è riusabile: la verifica provider decide.
        let mut pending=third.clone();let found=riusi(&base,&base.join("3"),&mut pending,|_|false);assert!(found.is_empty());
        // Senza la firma privata non si attribuisce una ricevuta a un altro download.
        std::fs::remove_file(base.join("3/.web-request")).unwrap();assert!(riusi(&base,&base.join("3"),&mut pending,|_|true).is_empty());
        std::fs::remove_dir_all(root).unwrap();
    }
}
