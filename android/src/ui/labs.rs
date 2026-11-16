//! Stato e controlli del laboratorio, senza I/O nel disegno.
use super::*;
use super::casa::telefono::{testo_a, altezza_a};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LabAz { Caso, SalvaCaso, TogliCaso, ChiudiCaso, Quaderno, Anteprima(u8, i32), AnnullaAnteprima, Lavori, TuttiLavori, File(usize), Tipo(bool) }

#[derive(Default)]
pub struct Laboratorio {
    pub caso: Option<cedro::caso::Caso>,
    pub codice: String,
    pub nota: String,
    pub caso_aperto: bool,
    pub filtro: Option<String>,
    pub focus: Option<PathBuf>,
    pub casi: std::collections::HashMap<u32,String>,
    pub path: Option<PathBuf>,
    pub scelta: crate::labs::Scelta,
    pub info: Option<crate::labs::Info>,
    pub immagine: Option<Image>,
    pub errore: String,
    pub richiesta: Option<u32>,
    pub aggiorna: bool,
}

impl App {
    pub(super) fn azione_labs(&mut self, az: LabAz) -> Effetto {
        match az {
            LabAz::Caso => {
                self.labs.codice = self.labs.caso.as_ref().map(|c|c.code.clone()).unwrap_or_default();
                self.labs.nota = self.labs.caso.as_ref().map(|c|c.note.clone()).unwrap_or_default();
                self.labs.caso_aperto = true;
                self.scorri = 0.0;
            }
            LabAz::ChiudiCaso => { self.labs.caso_aperto=false; self.scorri=0.0; }
            LabAz::SalvaCaso | LabAz::TogliCaso => {
                self.labs.caso = if az==LabAz::TogliCaso { None } else { cedro::caso::Caso::new(&self.labs.codice,&self.labs.nota) };
                self.labs.caso_aperto=false;
                self.scorri=0.0;
                return Effetto::SalvaCaso;
            }
            LabAz::Tipo(dicom) => {
                let path=self.casa.coda.iter().find(|v|v.scelto&&if dicom{crate::labs::dicom(&v.path)}else{crate::labs::vetrino(&v.path)||crate::labs::micro(&v.path)}).map(|v|v.path.clone());
                if let Some(path)=path {self.labs.focus=Some(path);self.labs.aggiorna=true;}
                else {return Effetto::Scegli(Sorgente::Coda);}
            }
            LabAz::File(i) => {self.labs.focus=self.casa.coda.get(i).map(|v|v.path.clone());self.labs.aggiorna=true;}
            LabAz::Lavori => {self.labs.filtro=self.labs.caso.as_ref().map(|c|c.code.clone());self.labs.caso_aperto=false;self.sopra=Some(Scheda::Lavori);self.scorri=0.0;}
            LabAz::TuttiLavori => {self.labs.filtro=None;self.scorri=0.0;}
            LabAz::Quaderno => return Effetto::Copia(self.quaderno()),
            LabAz::AnnullaAnteprima => {
                if let Some(id)=self.labs.richiesta.take() { return Effetto::Annulla(id); }
            }
            LabAz::Anteprima(asse,passo) => {
                let info=self.labs.info.clone().unwrap_or_default();
                let s=&mut self.labs.scelta;
                let muovi=|old:usize,n:usize| (old as i64+passo as i64).clamp(0,n.saturating_sub(1) as i64) as usize;
                match asse {
                    0 => { let k=s.livello.map(|n|n+1).unwrap_or(0); let n=muovi(k,info.livelli.len()+1); s.livello=n.checked_sub(1); }
                    1 => { s.scena=muovi(s.scena,info.scene.len()); s.canale=None; s.z=0; s.t=0; }
                    2 => { let n=muovi(s.canale.map(|n|n+1).unwrap_or(0),info.canali.len()+1); s.canale=n.checked_sub(1); }
                    3 => s.z=muovi(s.z,info.z),
                    4 => s.frame=muovi(s.frame,info.frames),
                    5 => s.finestra=muovi(s.finestra as usize,4) as u32,
                    6 => s.t=muovi(s.t,info.t),
                    _ => {},
                }
                self.labs.aggiorna=true;
            }
        }
        Effetto::Ridisegna
    }

    pub fn nome_esportato(&mut self,id:u32,nome:String)->String {
        let Some(c)=&self.labs.caso else{return nome};
        self.labs.casi.insert(id,c.code.clone());
        let p=Path::new(&nome);
        let stem=p.file_stem().and_then(|s|s.to_str()).unwrap_or(&nome);
        let ext=p.extension().and_then(|s|s.to_str()).unwrap_or("");
        let modello=if self.modello_nome.trim().is_empty(){cedro::caso::MODELLO}else{&self.modello_nome};
        let pattern=modello.replace("%caso%",&c.code).replace("%case%",&c.code);
        let name=cedro::name_example(&pattern,stem,ext).unwrap_or_else(||stem.into());
        if ext.is_empty(){name}else{format!("{name}.{ext}")}
    }

    pub fn quaderno(&self) -> String {
        let caso=self.labs.caso.as_ref().map(|c|c.code.as_str());
        let lavori:Vec<_>=self.lavori.iter().filter_map(|l|{
            let Stato::Fatto{nome,..}=&l.stato else {return None};
            let code=self.labs.casi.get(&l.id).cloned().unwrap_or_default();
            if caso.is_some_and(|c|c!=code) {return None;}
            let outputs=self.uscite.get(&l.id).map(|v|v.iter().map(|o|Path::new(&o.1).file_name().map(|n|n.to_string_lossy().into_owned()).unwrap_or_else(||o.1.clone())).collect()).unwrap_or_else(||vec![nome.clone()]);
            Some(cedro::caso::Lavoro{when:l.quando,what:l.etichetta.clone(),caso:code,outputs})
        }).collect();
        let mut lavori=lavori; lavori.reverse();
        cedro::caso::di_oggi(&lavori).into_iter().map(|l|l.per_quaderno()).collect::<Vec<_>>().join("\n\n")
    }

    /// Il glue chiama questo fuori dal disegno; ogni selezione ha un'identità propria.
    pub fn labs_visibile(&self)->bool {
        !self.labs.caso_aperto && self.opzioni.is_none() && ((self.casa_aperta && self.casa.laboratorio && self.sopra.is_none()) || (!self.casa_aperta && self.scheda==Scheda::Ruota && self.file.as_ref().is_some_and(|f|crate::labs::laboratorio(&f.path))))
    }

    /// Restituisce prima l'identità da cancellare, poi l'eventuale nuova richiesta.
    /// Vale anche quando la pagina resta aperta ma non contiene più una sorgente.
    pub fn sincronizza_labs(&mut self) -> (Option<u32>,Option<(u32,PathBuf,crate::labs::Scelta)>) {
        let prima=self.labs.richiesta;
        let nuova=self.richiesta_labs();
        (prima.filter(|id|Some(*id)!=self.labs.richiesta),nuova)
    }

    fn svuota_anteprima_labs(&mut self) {
        self.labs.path=None;
        self.labs.focus=None;
        self.labs.richiesta=None;
        self.labs.scelta=Default::default();
        self.labs.info=None;
        self.labs.immagine=None;
        self.labs.errore.clear();
        self.labs.aggiorna=false;
    }

    pub fn richiesta_labs(&mut self) -> Option<(u32,PathBuf,crate::labs::Scelta)> {
        if !self.labs_visibile(){self.svuota_anteprima_labs();return None;}
        let path=if self.casa_aperta && self.casa.laboratorio && self.sopra.is_none() {
            self.casa.coda.iter().filter(|v|v.scelto&&v.kind==self.casa.categoria&&crate::labs::laboratorio(&v.path)).min_by_key(|v|self.labs.focus.as_ref()!=Some(&v.path)).map(|v|v.path.clone())
        } else if !self.casa_aperta {self.file.as_ref().filter(|f|crate::labs::laboratorio(&f.path)).map(|f|f.path.clone())} else {None};
        let Some(path)=path else {self.svuota_anteprima_labs();return None;};
        if self.labs.path.as_ref()!=Some(&path) {
            self.labs.path=Some(path.clone());
            self.labs.scelta=Default::default();
            self.labs.info=None;
            self.labs.immagine=None;
            self.labs.aggiorna=true;
        }
        if !std::mem::take(&mut self.labs.aggiorna) {return None;}
        let id=self.prossimo;self.prossimo+=1;
        self.labs.richiesta=Some(id);
        self.labs.errore.clear();
        Some((id,path,self.labs.scelta.clone()))
    }

    pub fn ricevi_labs(&mut self,id:u32,result:Result<(crate::labs::Info,Image),String>) {
        if self.labs.richiesta!=Some(id) {return;}
        self.labs.richiesta=None;
        match result {
            Ok((info,img))=>{self.labs.info=Some(info);self.labs.immagine=Some(img);self.labs.errore.clear();},
            Err(e)=>{self.labs.errore=e;self.labs.immagine=None;},
        }
    }

    fn lab_bottone(&mut self,p:&mut Painter,x:f32,y:f32,w:f32,titolo:&str,ico:&str,az:Azione,vista:R)->f32 {
        let pal=self.pal();
        let h=(altezza_a(p,titolo,15.0,Weight::Semibold,w-60.0,20.0,0)+24.0).max(48.0);
        self.bottone_a(p,(x,y,w,h),titolo,ico,pal.surface,pal.primary,az,vista);
        h+8.0
    }

    pub(super) fn pannello_labs(&mut self,p:&mut Painter,x:f32,mut y:f32,w:f32,vista:R)->f32 {
        let pal=self.pal();
        let caso=self.labs.caso.as_ref().map(|c|format!("{} · {}",cedro::i18n::tr("Caso o campione"),c.code)).unwrap_or_else(||cedro::i18n::tr("+ Caso o campione").into_owned());
        y+=self.lab_bottone(p,x,y,w,&cedro::i18n::lit(&caso),ic::ADD,Azione::Labs(LabAz::Caso),vista);
        let bw=(w-8.0)/2.0;
        for (i,titolo) in ["Vetrini","DICOM"].into_iter().enumerate(){
            let on=self.labs.path.as_ref().is_some_and(|p|crate::labs::dicom(p))==(i==1);
            self.bottone_a(p,(x+i as f32*(bw+8.0),y,bw,48.0),titolo,"",if on{pal.alt}else{pal.surface},pal.primary,Azione::Labs(LabAz::Tipo(i==1)),vista);
        }
        y+=56.0;
        y+=self.lab_bottone(p,x,y,w,"Apri file",ic::ADD,Azione::Sorgente(Sorgente::Coda),vista);
        y+=self.lab_bottone(p,x,y,w,"Apri cartella MRXS",ic::FOLDER,Azione::Sorgente(Sorgente::Cartella),vista);
        if let Some(path)=&self.labs.path {
            let name=self.casa.coda.iter().find(|v|&v.path==path).map(|v|v.nome.clone()).unwrap_or_default();
            y+=testo_a(p,&cedro::i18n::lit(&name),x,y,w,16.0,Weight::Bold,pal.text,22.0,0)+12.0;
        }
        if let Some(img)=&self.labs.immagine {
            p.rrect(x,y,w,240.0,12.0,Some(pal.surface),None);
            p.image(img,x+6.0,y+6.0,w-12.0,228.0,8.0,Fit::Contain);y+=250.0;
        }
        if self.labs.richiesta.is_some() {
            self.bottone_a(p,(x,y,w,48.0),"Annulla anteprima",ic::CLOSE,pal.alt,pal.primary,Azione::Labs(LabAz::AnnullaAnteprima),vista);y+=56.0;
        }
        if !self.labs.errore.is_empty(){y+=testo_a(p,&self.labs.errore,x,y,w,14.0,Weight::Regular,pal.bad,20.0,0)+12.0;}
        if let Some(info)=self.labs.info.clone() {
            y+=testo_a(p,&cedro::i18n::lit(&info.descrizione),x,y,w,13.0,Weight::Regular,pal.muted,19.0,0)+8.0;
            y+=self.titolo_a(p,x,y,w,"Anteprima");
            let s=self.labs.scelta.clone();
            let mut assi=Vec::new();
            if !info.livelli.is_empty(){
                let valore=s.livello.and_then(|k|info.livelli.get(k).map(|(w,h)|format!("{} · {w} × {h}",k+1))).unwrap_or_else(||cedro::i18n::tr("Panoramica").into_owned());
                assi.push((0,"Livello",valore));
            }
            if !info.scene.is_empty(){assi.push((1,"Scena",format!("{} / {}",s.scena+1,info.scene.len())));}
            if !info.canali.is_empty(){assi.push((2,"Canale",s.canale.and_then(|k|info.canali.get(k).cloned()).unwrap_or_else(||cedro::i18n::tr("Tutti").into_owned())));}
            if info.z>0{assi.push((3,"Piano Z",format!("{} / {}",s.z+1,info.z)));}
            if info.t>1{assi.push((6,"Istante",format!("{} / {}",s.t+1,info.t)));}
            if info.frames>0{
                assi.push((4,"Immagine",format!("{} / {}",s.frame+1,info.frames)));
                assi.push((5,"Finestra",cedro::i18n::tr(["Del file","Polmone","Osso","Tessuti molli"][s.finestra.min(3) as usize]).into_owned()));
            }
            for (asse,titolo,valore) in assi {
                y+=testo_a(p,titolo,x,y,w,14.0,Weight::Semibold,pal.text,20.0,0)+6.0;
                let th=altezza_a(p,&cedro::i18n::lit(&valore),14.0,Weight::Regular,w-112.0,20.0,0).max(48.0);
                self.bottone_a(p,(x,y,48.0,th),"−","",pal.surface,pal.primary,Azione::Labs(LabAz::Anteprima(asse,-1)),vista);
                testo_a(p,&cedro::i18n::lit(&valore),x+56.0,y+8.0,w-112.0,14.0,Weight::Regular,pal.text,20.0,0);
                self.bottone_a(p,(x+w-48.0,y,48.0,th),"+","",pal.surface,pal.primary,Azione::Labs(LabAz::Anteprima(asse,1)),vista);y+=th+12.0;
            }
            if !info.avviso.is_empty(){y+=testo_a(p,&info.avviso,x,y,w,13.0,Weight::Regular,self.avviso(),19.0,0)+12.0;}
            y+=testo_a(p,"I selettori cambiano solo l’anteprima. L’esportazione usa le opzioni del risultato e include tutte le scene del file.",x,y,w,13.0,Weight::Regular,pal.muted,19.0,0)+12.0;
        }
        for (titolo,az) in [("Pacchetto con impronte",Azione::Pacchetto),("Lavori del caso",Azione::Labs(LabAz::Lavori)),("Copia per il quaderno",Azione::Labs(LabAz::Quaderno))] {
            y+=self.lab_bottone(p,x,y,w,titolo,ic::COPY,az,vista);
        }
        y
    }

    pub(super) fn pagina_caso(&mut self,p:&mut Painter,w:f32,h:f32) {
        let pal=self.pal();
        self.bottone_icona(p,(4.0,4.0,48.0,48.0),ic::ARROW_LEFT,Azione::Labs(LabAz::ChiudiCaso));
        p.text("Caso o campione",60.0,0.0,w-76.0,56.0,20.0,Weight::Bold,pal.text,Align::Left);
        self.pdf.campo_vis=None;
        let vista=(0.0,56.0,w,h-122.0);p.push_clip(vista.0,vista.1,vista.2,vista.3);
        let x=16.0;let cw=w-32.0;let mut y=72.0-self.scorri;
        y+=testo_a(p,"Un codice comune per i file del laboratorio.",x,y,cw,20.0,Weight::Bold,pal.text,26.0,0)+20.0;
        let code=self.labs.codice.clone();let note=self.labs.nota.clone();
        for (titolo,campo,valore) in [("Codice",Campo::CasoCodice,code),("Nota · resta nel caso, non nel nome del file",Campo::CasoNota,note)] {
            y+=testo_a(p,titolo,x,y,cw,14.0,Weight::Semibold,pal.muted,20.0,0)+6.0;
            self.campo_testo(p,(x,y,cw,56.0),campo,&valore,"","",false,vista);y+=72.0;
        }
        if let Some(c)=cedro::caso::Caso::new(&self.labs.codice,&self.labs.nota) {
            let modello=if self.modello_nome.trim().is_empty(){cedro::caso::MODELLO}else{&self.modello_nome};
            let esempio=format!("{}.png",cedro::name_example(&modello.replace("%caso%",&c.code),"file","png").unwrap_or_default());
            y+=testo_a(p,"Nome del risultato",x,y,cw,14.0,Weight::Semibold,pal.muted,20.0,0)+6.0;
            y+=testo_a(p,&cedro::i18n::lit(&esempio),x,y,cw,14.0,Weight::Regular,pal.text,20.0,0)+16.0;
        }
        for (titolo,az) in [("Usa questo caso",LabAz::SalvaCaso),("Togli caso attivo",LabAz::TogliCaso)]{
            y+=self.lab_bottone(p,x,y,cw,titolo,ic::CHECK,Azione::Labs(az),vista);
        }
        y+=self.lab_bottone(p,x,y,cw,"Lavori del caso",ic::CLOCK,Azione::Labs(LabAz::Lavori),vista);
        p.pop_clip();self.scorri_max=(y+self.scorri+16.0-(h-66.0)).max(0.0);self.tieni_campo(vista);
        self.barra_cedro(p,w,h,Some(0));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cedro::ui::gfx::{Fonts,GlyphCache};

    #[test]
    fn caso_snapshot_nomi_e_quaderno_restano_dopo_riavvio() {
        let mut app=App::new(ThemeId::Cedro);
        app.labs.codice="V/2026-0457".into();app.labs.nota="nota privata".into();
        assert!(matches!(app.azione_labs(LabAz::SalvaCaso),Effetto::SalvaCaso));
        app.imposta_file(Archivio{path:"/tmp/vetrino.svs".into(),nome:"vetrino.svs".into(),dim:1},Kind::Image,false);
        let Effetto::Converti{id,opts:Some(opts),..}=app.avvia("png","PNG",true) else{panic!("conversione assente")};
        assert_eq!(opts.caso.as_deref(),Some("V-2026-0457"));
        assert_eq!(opts.name_template.as_deref(),Some(cedro::caso::MODELLO));
        assert!(!serde_json::to_string(&opts).unwrap().contains("nota privata"));
        app.labs.caso=None;
        app.completa(Completamento{batch:None,id,stato:Stato::Fatto{path:"/tmp/a.png".into(),nome:"a.png".into(),msg:"ok".into()},uscite:Some(vec![("/tmp/a.png".into(),"a.png".into(),None),("/tmp/b.png".into(),"b.png".into(),None)]),sorgente:None});
        let identity=app.identita(id).unwrap();assert_eq!(identity.caso,"V-2026-0457");
        let json=app.serializza();assert!(!json.contains("nota privata"));
        let mut back=App::new(ThemeId::Cedro);back.carica_lavori(&json);
        assert_eq!(back.quaderno(),"Caso V-2026-0457\na.png\nb.png");
        back.labs.caso=cedro::caso::Caso::new("ALTRO","riservato");
        assert!(back.quaderno().is_empty());
        let nome=back.nome_esportato(99,"rapporto.txt".into());
        assert!(nome.contains("ALTRO_rapporto.txt")&&!nome.contains("riservato"));
        back.azione_labs(LabAz::TogliCaso);
        assert_eq!(back.nome_esportato(100,"rapporto.txt".into()),"rapporto.txt");
    }

    #[test]
    fn selettori_anteprima_non_modificano_export_ne_accettano_esiti_vecchi() {
        let mut a=App::new(ThemeId::Cedro);a.vai_casa();a.vista_nav(6);
        a.aggiungi_coda(vec![("/tmp/a.czi".into(),"a.czi".into(),Kind::Image)]);
        let (id,_,_)=a.richiesta_labs().unwrap();
        a.labs.info=Some(crate::labs::Info{scene:vec!["uno".into(),"due".into()],canali:vec!["A".into(),"B".into()],z:4,..Default::default()});
        a.azione_labs(LabAz::Anteprima(1,1));a.azione_labs(LabAz::Anteprima(2,1));a.azione_labs(LabAz::Anteprima(3,1));
        let (next,_,s)=a.richiesta_labs().unwrap();assert_ne!(id,next);
        assert_eq!((s.scena,s.canale,s.z),(1,Some(0),1));
        a.ricevi_labs(id,Err("vecchio".into()));assert!(a.labs.errore.is_empty());
        a.ricevi_labs(next,Err("corrente".into()));assert_eq!(a.labs.errore,"corrente");
        a.file=Some(Archivio{path:"/tmp/a.czi".into(),nome:"a.czi".into(),dim:1});
        let Effetto::Converti{opts:Some(opts),..}=a.avvia("png","PNG",true) else{panic!()};
        assert_eq!(opts.micro_channels,cedro::Options::default().micro_channels);
        assert_eq!(opts.micro_z,cedro::Options::default().micro_z);
    }

    #[test]
    fn sorgente_assente_cancella_preview_e_rifiuta_esiti_tardivi() {
        for modo in 0..4 {
            let mut a=App::new(ThemeId::Cedro);a.vai_casa();a.vista_nav(6);
            a.aggiungi_coda(vec![("/tmp/a.czi".into(),"a.czi".into(),Kind::Image)]);
            let (_,r)=a.sincronizza_labs();let (id,_,_)=r.unwrap();
            let img=Image::from_rgba(1,1,&[255,0,0,255]).unwrap();
            a.labs.info=Some(Default::default());a.labs.immagine=Some(img.clone());
            a.labs.errore="precedente".into();a.labs.scelta.z=3;
            match modo {
                0=>{a.casa.coda.clear();},
                1=>{a.casa.coda[0].scelto=false;},
                2=>{a.casa.categoria=Kind::Text;},
                _=>{a.sopra=Some(Scheda::Lavori);},
            }
            let (annulla,r)=a.sincronizza_labs();
            assert_eq!(annulla,Some(id));assert!(r.is_none());
            assert!(a.labs.path.is_none()&&a.labs.richiesta.is_none()&&a.labs.info.is_none()&&a.labs.immagine.is_none());
            assert!(a.labs.errore.is_empty());assert_eq!(a.labs.scelta.z,0);
            a.ricevi_labs(id,Ok((Default::default(),img)));
            assert!(a.labs.info.is_none()&&a.labs.immagine.is_none());
            a.ricevi_labs(id,Err("esito tardivo".into()));assert!(a.labs.errore.is_empty());
            let (annulla,r)=a.sincronizza_labs();assert!(annulla.is_none()&&r.is_none());
            a.sopra=None;a.casa.categoria=Kind::Image;
            if a.casa.coda.is_empty(){a.aggiungi_coda(vec![("/tmp/a.czi".into(),"a.czi".into(),Kind::Image)]);}else{a.casa.coda[0].scelto=true;}
            let (_,r)=a.sincronizza_labs();assert_ne!(r.unwrap().0,id);
        }
    }

    #[test]
    fn risultato_mostra_il_tempo_senza_coprire_avviso_e_azioni() {
        let mut a=App::new(ThemeId::Cedro);a.anim.calmo=true;a.grandi=true;
        a.lavori.push(Lavoro{id:1,origine:"campione".into(),etichetta:"PNG".into(),conv:true,inizio:Instant::now(),quando:adesso(),stato:Stato::Fatto{path:"/tmp/a.png".into(),nome:"a.png".into(),msg:"Attenzione: messaggio lungo su più righe per il risultato del campione".into()},prog:Default::default(),sorgente:None,controlli:None});
        a.labs.casi.insert(1,"CASO".into());
        let mut cache=GlyphCache::new();
        let mut p=Painter::new(360,900,1.0,Fonts::for_theme(a.tema),&mut cache);p.larger=true;
        let h=a.scheda_lavoro(&mut p,0,16.0,0.0,328.0,(0.0,0.0,360.0,900.0));
        let prima=p.pixmap.data().to_vec();drop(p);
        a.lavori[0].quando=adesso()-864000;
        let mut p=Painter::new(360,900,1.0,Fonts::for_theme(a.tema),&mut cache);p.larger=true;
        let dopo=a.scheda_lavoro(&mut p,0,16.0,0.0,328.0,(0.0,0.0,360.0,900.0));
        assert_eq!(h,dopo);assert!(prima.as_slice()!=p.pixmap.data(),"il riferimento temporale deve cambiare i pixel");
        let msg=match &a.lavori[0].stato{Stato::Fatto{msg,..}=>msg,_=>unreachable!()};
        let note=altezza_a(&p,&quando(a.lavori[0].quando),13.0,Weight::Regular,300.0,19.0,0)+6.0+altezza_a(&p,msg,13.0,Weight::Regular,300.0,19.0,0)+6.0;
        let mut azioni=0;
        for (forma,az) in &a.colpi {
            if matches!(az,Azione::Apri(1)|Azione::Condividi(1)|Azione::CopiaRapporto(1)) {
                azioni+=1;
                let Forma::R(r)=forma else{panic!()};assert!(r.1>=54.0+note+32.0+22.0);assert!(r.3>=48.0&&r.1+r.3<=h);
            }
        }
        assert_eq!(azioni,6); // tre azioni per ciascuno dei due disegni della scheda.
    }

    #[test]
    fn labs_e_caso_hanno_controlli_raggiungibili_scorrendo() {
        let mut a=App::new(ThemeId::Cedro);a.vai_casa();a.vista_nav(6);
        a.aggiungi_coda(vec![("/tmp/a.czi".into(),"a.czi".into(),Kind::Image)]);
        a.labs.info=Some(crate::labs::Info{scene:vec!["A".into(),"B".into()],canali:vec!["A".into()],z:3,t:2,..Default::default()});
        a.grandi=true;a.anim.calmo=true;
        let mut cache=GlyphCache::new();let mut seen=Vec::new();let mut scroll=0.0;
        loop {
            a.scorri=scroll;
            let mut p=Painter::new(360,640,1.0,Fonts::for_theme(a.tema),&mut cache);p.larger=true;
            a.disegna(&mut p,360.0,640.0);
            for (shape,az) in &a.colpi {
                if let (Forma::R(r),Azione::Labs(lab))=(shape,az) {
                    if r.2<48.0 || r.3<48.0 {continue;}
                    if !seen.contains(lab){seen.push(*lab);}
                }
            }
            if scroll>=a.scorri_max{break;}scroll=(scroll+240.0).min(a.scorri_max);
        }
        for az in [LabAz::Caso,LabAz::Quaderno,LabAz::Anteprima(1,1),LabAz::Anteprima(2,1),LabAz::Anteprima(3,1),LabAz::Anteprima(6,1)]{assert!(seen.contains(&az),"{az:?}");}
        a.azione_labs(LabAz::Caso);
        let mut p=Painter::new(360,640,1.0,Fonts::for_theme(a.tema),&mut cache);a.disegna(&mut p,360.0,640.0);
        assert!(a.colpi.iter().any(|(_,az)|*az==Azione::Campo(Campo::CasoCodice)));
    }

    #[test]
    fn catalogo_completo_corrisponde_al_motore_e_avvisi_non_spariscono() {
        for k in [Kind::Image,Kind::Doc,Kind::Text,Kind::Pdf,Kind::Audio,Kind::Video,Kind::Archive] {
            let ids:Vec<_>=righe(k).into_iter().map(|r|r.id).collect();
            for s in cedro::panel_slices(k).iter().chain(cedro::slices(k,Mode::Tools)) {
                if s.id!="none"{assert!(ids.contains(&s.id),"{k:?}: {}",s.id);}
            }
        }
        assert!(avviso_motore("Attenzione: il testo da destra a sinistra (arabo, ebraico) non è ancora reso bene"));
        assert!(avviso_motore("Gli allegati (2) restano solo nel file originale: non sono stati copiati"));
        assert!(!avviso_motore("1 pagina"));
    }
    #[test]
    fn pacchetto_include_anche_i_dati_di_un_altra_categoria() {
        let mut a=App::new(ThemeId::Cedro);a.vai_casa();a.vista_nav(6);
        a.aggiungi_coda(vec![("/tmp/a.svs".into(),"a.svs".into(),Kind::Image),("/tmp/dati.csv".into(),"dati.csv".into(),Kind::Text)]);
        let Effetto::Converti{azione,unisci,..}=a.pacchetto_labs()else{panic!()};
        assert_eq!(azione,"package");assert_eq!(unisci.len(),2);
    }

}
