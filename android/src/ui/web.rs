//! Le scelte web nello stesso linguaggio della variante A.
use super::*;
use super::casa::telefono::altezza_a;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum WebAz {Rete(u8),Altezza(u32),Audio(u32),Formato(&'static str),Opzione(u8),Sottotitoli(u32),Svuota}
impl App {
    pub fn ricevi_batch(&mut self,batch:&crate::web::Batch)->Vec<(u32,usize,Arc<AtomicU32>)>{
        let mut tasks=Vec::new();
        for r in batch.ricevute.iter().cloned(){self.recupera(r);}
        for (index,child) in batch.figli.iter().enumerate(){
            if self.lavori_rimossi.contains(&child.identita.chiave){continue;}
            // La stessa consegna ripetuta non avvia due motori sullo stesso audio.
            let existing=self.chiavi_lavori.iter().find(|(_,k)|*k==&child.identita.chiave).map(|(id,_)|*id);
            if child.fine&&existing.is_some(){continue;}
            if existing.is_some_and(|id|self.lavori.iter().any(|l|l.id==id&&matches!(l.stato,Stato::InCorso|Stato::Fatto{..}))){continue;}
            let (id,prog)=if let Some(id)=existing{let l=self.lavori.iter_mut().find(|l|l.id==id).unwrap();l.stato=Stato::InCorso;(id,l.prog.clone())}else{self.nuova_importazione()};
            self.chiavi_lavori.insert(id,child.identita.chiave.clone());
            if !child.identita.caso.is_empty(){self.labs.casi.insert(id,child.identita.caso.clone());}
            if let Some(l)=self.lavori.iter_mut().find(|l|l.id==id){l.origine=child.identita.origine.clone();l.etichetta=child.identita.etichetta.clone();l.quando=child.identita.quando;l.sorgente=Some(child.source.clone());}
            if child.fine{if let Some(l)=self.lavori.iter_mut().find(|l|l.id==id){l.stato=Stato::Errore("Audio disponibile: avvia di nuovo la trascrizione".into());}}else{tasks.push((id,index,prog));}
        }tasks
    }
    pub(super) fn azione_web(&mut self,az:WebAz)->Effetto{
        match az {
            WebAz::Rete(n)=>{self.rete=n;return Effetto::SalvaRete;},
            WebAz::Altezza(n)=>self.web.altezza=n,
            WebAz::Audio(n)=>self.web.audio_quality=n,
            WebAz::Formato(s)=>if matches!(self.casa.web,1|3){self.web.audio_format=s.into()}else{self.web.video_format=s.into()},
            WebAz::Sottotitoli(n)=>self.web.subtitles=n,
            WebAz::Svuota=>self.casa.link.clear(),
            WebAz::Opzione(n)=>{let w=&mut self.web;let v=match n{0=>&mut w.thumbnail,1=>&mut w.chapters,2=>&mut w.metadata,3=>&mut w.split_chapters,4=>&mut w.skip_sponsors,5=>&mut w.normalize,6=>&mut w.playlist,_=>&mut w.skip_existing};*v=!*v;},
        }
        Effetto::Ridisegna
    }
    fn scelta_web(&mut self,p:&mut Painter,r:R,label:&str,selected:bool,enabled:bool,az:WebAz,vista:R){
        let pal=self.pal();
        self.bottone_a(p,r,label,if selected{ic::CHECK}else{""},if selected&&enabled{pal.alt}else{pal.surface},if enabled{pal.text}else{pal.muted},if enabled{Azione::Web(az)}else{Azione::Niente},vista);
    }
    pub(super) fn pannello_web(&mut self,p:&mut Painter,x:f32,mut y:f32,w:f32,vista:R)->f32{
        let start=y;let pal=self.pal();let audio=matches!(self.casa.web,1|3);
        let youtube=cedro::youtube::is_youtube_url(self.casa.link.trim());
        let enabled=youtube&&cfg!(feature="youtube");
        if !youtube{self.web.video_format="mp4".into();if self.web.audio_format=="m4a"{self.web.audio_format="mp3".into();}}
        if youtube&&!cfg!(feature="youtube") {
            y+=p.text_wrap("Questa versione passa il link a un'altra app.",x,y,w,14.0,Weight::Regular,pal.muted,21.0,0)+16.0;
            return y-start;
        }
        self.scelta_web(p,(x,y,w,48.0),"Svuota",false,true,WebAz::Svuota,vista);y+=64.0;
        y+=self.titolo_a(p,x,y,w,if audio{"Qualità audio"}else{"Qualità del video"});
        let tw=(w-8.0)/2.0;
        let choices:Vec<_>=if audio{
            ["Massima","Buona · consigliata","Leggera"].into_iter().enumerate().map(|(i,label)|(WebAz::Audio(i as u32),label,self.web.audio_quality==i as u32)).collect()
        }else{
            [(0,"Originale"),(1080,"1080p · consigliata"),(720,"720p"),(480,"480p")].into_iter().map(|(n,label)|(WebAz::Altezza(n),label,self.web.altezza==n)).collect()
        };
        let qh=choices.iter().map(|(_,label,_)|altezza_a(p,label,15.0,Weight::Semibold,tw-58.0,20.0,0)+24.0).fold(68.0,f32::max);
        for (i,(az,label,on)) in choices.into_iter().enumerate(){self.scelta_web(p,(x+(i%2) as f32*(tw+8.0),y+(i/2) as f32*(qh+8.0),tw,qh),label,on,true,az,vista);}
        y+=2.0*(qh+8.0)+12.0;
        if !youtube {
            y+=p.text_wrap("Scarica l'originale, poi riduce o converte il file sul telefono.",x,y,w,14.0,Weight::Regular,pal.muted,21.0,0)+16.0;
        }
        y+=self.titolo_a(p,x,y,w,"Formato");
        let formats:&[&str]=if audio{&["mp3","m4a","opus","wav","flac"]}else{&["mp4","mkv","webm"]};
        for (i,&f) in formats.iter().enumerate(){
            let selected=if audio{self.web.audio_format==f}else{self.web.video_format==f&&(enabled||self.web.altezza>0)};
            let supported=enabled||if audio{f!="m4a"}else{f=="mp4"&&self.web.altezza>0};
            self.scelta_web(p,(x+(i%2) as f32*(tw+8.0),y+(i/2) as f32*60.0,tw,52.0),f,selected,supported,WebAz::Formato(f),vista);
        }
        y+=((formats.len()+1)/2) as f32*60.0+16.0;
        if !youtube {y+=p.text_wrap("Copertina, capitoli, metadati, sottotitoli e raccolte richiedono YouTube. Per questi link sono disponibili MP4 e i formati audio indicati.",x,y,w,14.0,Weight::Regular,pal.muted,21.0,0)+16.0;}
        for (title,rows) in [
            ("Dentro il file",vec![(0,"Copertina",self.web.thumbnail,enabled),(1,"Capitoli",self.web.chapters,enabled),(2,"Metadati",self.web.metadata,enabled)]),
            ("Come lo salva",vec![(3,"Dividi per capitoli",self.web.split_chapters,enabled),(4,"Salta gli sponsor",self.web.skip_sponsors,enabled),(5,"Uniforma il volume",self.web.normalize,audio)]),
            ("Playlist e raccolte",vec![(6,"Scarica la raccolta",self.web.playlist,enabled),(7,"Evita copie già salvate",self.web.skip_existing,enabled)]),
        ] {
            y+=self.titolo_a(p,x,y,w,title);
            for (n,label,on,active) in rows {
                let hh=altezza_a(p,label,15.0,Weight::Semibold,w-58.0,21.0,0).max(28.0)+20.0;
                self.scelta_web(p,(x,y,w,hh),label,on,active,WebAz::Opzione(n),vista);y+=hh+8.0;
            }
            if title=="Dentro il file" {
                y+=self.titolo_a(p,x,y,w,"Sottotitoli, quando disponibili");
                for (i,label) in ["Nessuno","Italiano","Italiano e inglese","Tutte le lingue"].into_iter().enumerate(){
                    self.scelta_web(p,(x,y,w,52.0),label,self.web.subtitles==i as u32,enabled&&!audio,WebAz::Sottotitoli(i as u32),vista);y+=60.0;
                }
                y+=p.text_wrap("Un problema con i sottotitoli non interrompe il video.",x,y,w,14.0,Weight::Regular,pal.muted,21.0,0)+16.0;
            }
            if title=="Playlist e raccolte"{y+=p.text_wrap("Evita copie duplicate al salvataggio. Il download può avvenire comunque.",x,y,w,14.0,Weight::Regular,pal.muted,21.0,0)+12.0;}
            y+=16.0;
        }
        y+=self.riga_componente(p,x,y,w);
        y-start
    }
    pub(super) fn impostazioni_rete(&mut self,p:&mut Painter,x:f32,mut y:f32,w:f32,vista:R)->f32{
        let start=y;y+=self.titolo_a(p,x,y,w,"Download dei componenti");
        for (i,label) in ["Chiedi una volta su rete mobile","Solo Wi-Fi","Consenti anche rete mobile"].into_iter().enumerate(){
            let hh=altezza_a(p,label,15.0,Weight::Semibold,w-58.0,21.0,0).max(28.0)+20.0;
            self.scelta_web(p,(x,y,w,hh),label,self.rete==i as u8,true,WebAz::Rete(i as u8),vista);y+=hh+8.0;
        }y-start
    }
    pub(super) fn pannello_download(&mut self,p:&mut Painter,x:f32,mut y:f32,w:f32,vista:R)->f32{
        let Some(a)=self.download_attesa.clone() else{return 0.0};let start=y;let pal=self.pal();
        let label=format!("{} · {} · {}: {}",cedro::i18n::tr(&a.nome),cedro::size_words(a.bytes),cedro::i18n::tr("Spazio libero"),cedro::size_words(a.liberi));
        y+=p.text_wrap(&cedro::i18n::lit(&label),x,y,w,14.0,Weight::Semibold,pal.text,21.0,0)+12.0;
        if a.mobile&&self.rete!=2 {y+=self.impostazioni_rete(p,x,y,w,vista);}
        y-start
    }
    pub(super) fn riga_componente(&self,p:&mut Painter,x:f32,y:f32,w:f32)->f32 {
        let Some(c)=&self.componente else{return 0.0};let pal=self.pal();let text=c.testo();
        let hh=altezza_a(p,&cedro::i18n::lit(&text),13.0,Weight::Regular,w-24.0,19.0,0)+24.0;
        p.rrect(x,y,w,hh,10.0,Some(pal.alt),None);
        p.text_wrap(&cedro::i18n::lit(&text),x+12.0,y+12.0,w-24.0,13.0,Weight::Regular,pal.text,19.0,0);hh+12.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ui_snapshot_web_request_e_delega_catalogo(){
        let mut app=App::new(ThemeId::Cedro);app.vai_casa();app.vista_nav(4);
        app.casa.link="https://example.org/video.mp4".into();
        app.azione_web(WebAz::Altezza(720));app.azione_web(WebAz::Audio(2));
        app.azione_web(WebAz::Sottotitoli(2));app.azione_web(WebAz::Opzione(6));
        app.modello=2;app.voci=3;app.qualita=91;
        let Effetto::Converti{web:Some(scelte),..}=app.azione_casa(super::super::casa::CasaAz::Vai) else{panic!("richiesta assente")};
        app.azione_web(WebAz::Altezza(480));
        let request=scelte.richiesta("https://youtu.be/test","web-video","out".into());
        assert_eq!(request.quality,720);assert!(request.playlist);assert_eq!(request.subtitles,2);
        assert_eq!(scelte.media_quality,91);assert_eq!(scelte.voce,crate::worker::Voce{modello:2,voci:3,referto:false});
        app.casa.link="https://youtu.be/test".into();
        let Effetto::Converti{azione,..}=app.azione_casa(super::super::casa::CasaAz::Vai) else{panic!()};
        assert_eq!(azione,if cfg!(feature="youtube"){"web-video"}else{"web-altra-app"});
    }
    #[test]
    fn recupero_batch_con_mtime_riscritto_e_ricevuta_pronta(){
        use crate::publication::{Identita,Registro};
        let root=crate::storage::nuova_cartella(&std::env::temp_dir()).unwrap();
        let files=[root.join("a.wav"),root.join("b.wav")];
        for (i,file) in files.iter().enumerate(){std::fs::write(file,[i as u8;4]).unwrap();}
        let identity=Identita{chiave:"batch-mtime".into(),origine:"raccolta".into(),etichetta:"Testo".into(),conv:false,quando:1,sorgente:None,caso:"C-1".into()};
        let batch=crate::web::Batch::prepara(&root,&files,&identity,Default::default(),Default::default()).unwrap();
        let mut app=App::new(ThemeId::Cedro);let tasks=app.ricevi_batch(&batch);
        assert_eq!(tasks.len(),2);let before_completion=app.serializza();
        let child=&batch.figli[0];let txt=child.out.join("a.txt");std::fs::write(&txt,b"testo completato").unwrap();
        let mut receipt=Registro::prepara(&child.out,&txt,child.identita.clone(),"ok".into()).unwrap();
        receipt.salva(&child.out).unwrap();
        let published=crate::publication::legacy(&txt,&root.join("pubblici"),0,&std::sync::atomic::AtomicBool::new(false),&mut |d|receipt.prenota(&child.out,0,d)).unwrap();
        receipt.completato(&child.out,0,published.clone()).unwrap();
        // Crash dopo il commit della ricevuta, prima della consegna UI e di batch.finisci.
        let prima=std::time::SystemTime::now();
        std::thread::sleep(std::time::Duration::from_millis(20));
        let mut recovered=crate::web::Batch::leggi(&root).unwrap();recovered.materializza().unwrap();
        assert!(std::fs::metadata(&child.out).unwrap().modified().unwrap()>=prima);
        // Il vecchio filtro mtime < prima escluderebbe proprio questo figlio.
        recovered.recupera_ricevute(|dir|Registro::leggi(dir).ok());
        assert_eq!(recovered.ricevute.len(),1);
        let mut restarted=App::new(ThemeId::Cedro);restarted.carica_lavori(&before_completion);
        let pending=restarted.ricevi_batch(&recovered);assert_eq!(pending.len(),1);assert_eq!(pending[0].1,1);
        let id=*restarted.chiavi_lavori.iter().find(|(_,key)|*key==&child.identita.chiave).unwrap().0;
        assert!(matches!(&restarted.lavori.iter().find(|l|l.id==id).unwrap().stato,Stato::Fatto{path,..} if path==&published.path));
        assert_eq!(restarted.uscite[&id][0].0,published.path);
        assert_eq!(restarted.lavori.iter().find(|l|l.id==id).unwrap().sorgente.as_ref(),Some(&child.source));
        assert!(restarted.ricevi_batch(&recovered).is_empty());
        // Anche una ricevuta arrivata fra la registrazione e l'avvio produce un completamento.
        let mut late=App::new(ThemeId::Cedro);let late_tasks=late.ricevi_batch(&batch);
        late.completa(Completamento::recuperato(late_tasks[0].0,Registro::leggi(&child.out).unwrap()));
        assert!(matches!(late.lavori.iter().find(|l|l.id==late_tasks[0].0).unwrap().stato,Stato::Fatto{..}));
        for (i,child) in recovered.figli.iter().enumerate(){assert_eq!(std::fs::read(&child.source).unwrap(),[i as u8;4]);}
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn consegna_batch_ripetuta_crash_e_tombstone(){
        let identity=crate::publication::Identita{chiave:"batch/audio-0".into(),origine:"a.wav".into(),etichetta:"Testo".into(),conv:false,quando:1,sorgente:Some("/privato/0/.revisione/a.wav".into()),caso:"C-1".into()};
        let child=crate::web::Figlio{identita:identity,origine:"/privato/a.wav".into(),out:"/privato/0".into(),source:"/privato/0/.revisione/a.wav".into(),fine:false};
        let mut other=child.clone();other.identita.chiave="batch/audio-1".into();other.source="/privato/1/.revisione/b.wav".into();
        let batch=crate::web::Batch{ricevute:vec![],root:"/privato".into(),figli:vec![child,other],opts:Default::default(),voce:Default::default()};
        let mut app=App::new(ThemeId::Cedro);let tasks=app.ricevi_batch(&batch);assert_eq!(tasks.len(),2);
        assert!(app.ricevi_batch(&batch).is_empty());
        assert_ne!(app.lavori[0].sorgente,app.lavori[1].sorgente);
        let saved=app.serializza();let mut restarted=App::new(ThemeId::Cedro);restarted.carica_lavori(&saved);
        assert_eq!(restarted.ricevi_batch(&batch).len(),2);
        restarted.lavori.clear();restarted.lavori_rimossi.insert("batch/audio-0".into());restarted.chiavi_lavori.clear();
        let tasks=restarted.ricevi_batch(&batch);assert_eq!(tasks.len(),1);assert_eq!(tasks[0].1,1);
    }
}
