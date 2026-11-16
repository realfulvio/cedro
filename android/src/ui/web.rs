//! Le scelte web nello stesso linguaggio della variante A.
use super::*;
use super::casa::telefono::altezza_a;

#[derive(Clone,Copy,Debug,PartialEq,Eq)]
pub enum WebAz {Rete(u8),Conferma,Altezza(u32),Audio(u32),Formato(&'static str),Opzione(u8),Sottotitoli(u32),Svuota}
impl App {
    pub fn ricevi_batch(&mut self,batch:&crate::web::Batch)->Vec<(u32,usize,Arc<AtomicU32>)>{
        let mut tasks=Vec::new();
        for (index,child) in batch.figli.iter().enumerate(){
            if self.lavori_rimossi.contains(&child.identita.chiave){continue;}
            // La stessa consegna ripetuta non avvia due motori sullo stesso audio.
            let existing=self.chiavi_lavori.iter().find(|(_,k)|*k==&child.identita.chiave).map(|(id,_)|*id);
            if child.fine&&existing.is_some(){continue;}
            if existing.is_some_and(|id|self.lavori.iter().any(|l|l.id==id&&matches!(l.stato,Stato::InCorso|Stato::Fatto{..}))){continue;}
            let (id,prog)=if let Some(id)=existing{let l=self.lavori.iter_mut().find(|l|l.id==id).unwrap();l.stato=Stato::InCorso;(id,l.prog.clone())}else{self.nuova_importazione()};
            self.chiavi_lavori.insert(id,child.identita.chiave.clone());
            if !child.identita.caso.is_empty(){self.labs.casi.insert(id,child.identita.caso.clone());}
            if let Some(l)=self.lavori.iter_mut().find(|l|l.id==id){l.origine=child.identita.origine.clone();l.etichetta=child.identita.etichetta.clone();l.sorgente=Some(child.source.clone());}
            if child.fine{if let Some(l)=self.lavori.iter_mut().find(|l|l.id==id){l.stato=Stato::Errore("Audio disponibile: avvia di nuovo la trascrizione".into());}}else{tasks.push((id,index,prog));}
        }tasks
    }
    pub(super) fn azione_web(&mut self,az:WebAz)->Effetto{
        match az {
            WebAz::Rete(n)=>{self.rete=n;return Effetto::SalvaRete;},
            WebAz::Conferma=>return Effetto::ConfermaDownload,
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
        if audio{
            for (i,label) in ["Massima","Buona · consigliata","Leggera"].into_iter().enumerate(){
                self.scelta_web(p,(x+(i%2) as f32*(tw+8.0),y+(i/2) as f32*76.0,tw,68.0),label,self.web.audio_quality==i as u32,true,WebAz::Audio(i as u32),vista);
            }
        }else{
            for (i,(n,label)) in [(0,"Originale"),(1080,"1080p · consigliata"),(720,"720p"),(480,"480p")].into_iter().enumerate(){
                self.scelta_web(p,(x+(i%2) as f32*(tw+8.0),y+(i/2) as f32*76.0,tw,68.0),label,self.web.altezza==n,true,WebAz::Altezza(n),vista);
            }
        }
        y+=164.0;
        if !youtube {
            y+=p.text_wrap("Scarica l'originale, poi riduce o converte il file sul telefono.",x,y,w,14.0,Weight::Regular,pal.muted,21.0,0)+16.0;
        }
        y+=self.titolo_a(p,x,y,w,"Formato");
        let formats:&[&str]=if audio{&["mp3","m4a","opus","wav","flac"]}else{&["mp4","mkv","webm"]};
        for (i,&f) in formats.iter().enumerate(){
            let selected=if audio{self.web.audio_format==f}else{self.web.video_format==f};
            let supported=enabled||if audio{f!="m4a"}else{f=="mp4"};
            self.scelta_web(p,(x+(i%2) as f32*(tw+8.0),y+(i/2) as f32*60.0,tw,52.0),f,selected,supported,WebAz::Formato(f),vista);
        }
        y+=((formats.len()+1)/2) as f32*60.0+16.0;
        if !youtube {y+=p.text_wrap("Copertina, capitoli, metadati, sottotitoli e raccolte richiedono YouTube. Per questi link sono disponibili MP4 e i formati audio indicati.",x,y,w,14.0,Weight::Regular,pal.muted,21.0,0)+16.0;}
        for (title,rows) in [
            ("Dentro il file",vec![(0,"Copertina",self.web.thumbnail,enabled),(1,"Capitoli",self.web.chapters,enabled),(2,"Metadati",self.web.metadata,enabled)]),
            ("Come lo salva",vec![(3,"Dividi per capitoli",self.web.split_chapters,enabled),(4,"Salta gli sponsor",self.web.skip_sponsors,enabled),(5,"Uniforma il volume",self.web.normalize,audio)]),
            ("Playlist e raccolte",vec![(6,"Scarica la raccolta",self.web.playlist,enabled),(7,"Salta i file già presenti",self.web.skip_existing,enabled)]),
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
        if a.conferma_spazio{self.scelta_web(p,(x,y,w,52.0),"Scarica il componente",false,true,WebAz::Conferma,vista);y+=60.0;}
        y-start
    }
    pub(super) fn riga_componente(&self,p:&mut Painter,x:f32,y:f32,w:f32)->f32 {
        let Some(c)=&self.componente else{return 0.0};let pal=self.pal();let text=c.testo();
        let hh=altezza_a(p,&cedro::i18n::lit(&text),13.0,Weight::Regular,w-24.0,19.0,0)+24.0;
        p.rrect(x,y,w,hh,10.0,Some(pal.alt),None);
        p.text_wrap(&cedro::i18n::lit(&text),x+12.0,y+12.0,w-24.0,13.0,Weight::Regular,pal.text,19.0,0);hh+12.0
    }
}
