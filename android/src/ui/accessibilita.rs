//! Semantica della UI Rust, derivata dagli stessi bersagli realmente attivabili.
use super::*;
use std::hash::{Hash,Hasher};
impl App {
    /// Wire format emitted by CedroActivity.takeAccessibilityActions: [kind,x,y,endY] pixels.
    /// Called only on the Rust UI thread; effects join the physical input dispatcher in glue.
    pub fn azioni_accessibili(&mut self,json:&str,scale:f32)->Vec<Effetto> {
        let mut effects=Vec::new();
        if !scale.is_finite()||scale<=0.0{return effects;}
        let Ok(commands)=serde_json::from_str::<Vec<(u8,f32,f32,f32)>>(json) else{return effects};
        for (kind,x,y,end) in commands {
            if kind>1||![x,y,end].iter().all(|n|n.is_finite()){continue;}
            let (x,y,end)=(x/scale,y/scale,end/scale);
            effects.push(self.tocca_giu(x,y));
            if kind==1 {effects.push(self.tocca_muovi(x,end));}
            effects.push(self.tocca_su(x,if kind==1{end}else{y}));
        }
        effects
    }

    pub fn accessibilita(&self,p:&Painter)->String {
        let mut nodes=Vec::new();
        let texts=p.accessibility_text.as_deref().unwrap_or(&[]);
        let mut used=std::collections::HashSet::new();
        let mut occurrences=std::collections::HashMap::<String,u32>::new();
        let mut identifiers=std::collections::HashSet::new();
        for (shape,action) in &self.colpi {
            let key=format!("{action:?}");
            let occurrence=occurrences.entry(key.clone()).or_default();
            let region=*occurrence;*occurrence+=1;
            if matches!(action,Azione::Niente|Azione::Riv(rivedi::RivAz::Onda(_))){continue;}
            let rect=match shape {Forma::R(r)=>*r,Forma::Cerchio{cx,cy,r}=>(*cx-*r,*cy-*r,*r*2.0,*r*2.0),_=>continue};
            let bounds=[(rect.0+self.margini.0)*p.scale,(rect.1+self.margini.1)*p.scale,(rect.0+rect.2+self.margini.0)*p.scale,(rect.1+rect.3+self.margini.1)*p.scale];
            if bounds[2]<=0.0||bounds[3]<=0.0||bounds[0]>=p.pixmap.width() as f32||bounds[1]>=p.pixmap.height() as f32 {continue;}
            // Solo i bersagli raggiungibili: dialoghi e header coprono davvero i controlli sottostanti.
            if self.trova(rect.0+rect.2/2.0,rect.1+rect.3/2.0)!=Some(*action){continue;}
            let mut parts=Vec::new();
            for (i,(text,r)) in texts.iter().enumerate() {
                let (cx,cy)=((r[0]+r[2])/2.0,(r[1]+r[3])/2.0);
                if cx>=bounds[0]&&cx<=bounds[2]&&cy>=bounds[1]&&cy<=bounds[3] {
                    parts.push(text.clone());used.insert(i);
                }
            }
            let fallback=match action {
                Azione::Esci|Azione::ChiudiScelta=>"Chiudi",
                Azione::IndietroFoglio|Azione::Riv(rivedi::RivAz::Chiudi)|Azione::Casa(casa::CasaAz::Indietro)=>"Indietro",
                Azione::Casa(casa::CasaAz::Nav(0))=>"Inizio",
                Azione::Casa(casa::CasaAz::Aggiungi)|Azione::ApriScelta=>"Aggiungi file",
                Azione::Recente(_)|Azione::Casa(casa::CasaAz::RecenteVai(_))=>"Apri",
                Azione::CopiaErrore(_)=>"Copia errore",
                Azione::Riv(rivedi::RivAz::Ascolta(_))=>"Ascolta",
                _=>"",
            };
            let label=if parts.is_empty(){cedro::i18n::tr(fallback).into_owned()}else{parts.join(" · ")};
            if label.is_empty(){continue;}
            let selected=match action {
                Azione::Tema(i)=>TEMI.get(*i)==Some(&self.tema),
                Azione::Caratteri(v)=>self.grandi==*v,
                Azione::Lingua(i)=>LINGUE.get(*i).is_some_and(|v|v.0==self.lingua),
                Azione::Casa(casa::CasaAz::Referto)=>self.referto,
                Azione::Casa(casa::CasaAz::Vista(v))=>!self.imp.simple==*v,
                _=>false,
            };
            let checkable=matches!(action,Azione::Casa(casa::CasaAz::Referto));
            let mut hash=std::collections::hash_map::DefaultHasher::new();key.hash(&mut hash);region.hash(&mut hash);
            let mut id=(hash.finish()&0x1fffffff) as u32+1;
            while !identifiers.insert(id){id=if id==0x3fffffff{1}else{id+1};}
            nodes.push(serde_json::json!({"id":id,"text":label,"bounds":bounds,"click":true,"selected":selected,"checkable":checkable,"checked":selected}));
        }
        for (i,(text,bounds)) in texts.iter().enumerate().filter(|(i,_)|!used.contains(i)) {
            nodes.push(serde_json::json!({"id":0x40000000u32+i as u32,"text":text,"bounds":bounds,"click":false}));
        }
        serde_json::json!({"nodes":nodes,"scroll":self.scorri_max>0.0||self.rivedi.is_some()}).to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cedro::ui::gfx::{Fonts,GlyphCache};
    #[test]
    fn native_semantic_centers_dispatch_with_density_and_system_insets() {
        let mut cache=GlyphCache::new();
        for (w,h,margins) in [(360,640,(0.0,95.0/3.0,0.0,16.0)),(640,360,(83.0/3.0,0.0,16.0,0.0))] {
            let mut app=App::new(ThemeId::Cedro);app.margini=margins;
            let mut p=Painter::new(w*3,h*3,3.0,Fonts::for_theme(app.tema),&mut cache);
            p.accessibility_text=Some(Vec::new());app.disegna(&mut p,w as f32,h as f32);
            let data:serde_json::Value=serde_json::from_str(&app.accessibilita(&p)).unwrap();
            let job=data["nodes"].as_array().unwrap().iter().find(|n|n["text"]==cedro::i18n::tr("Lavori").as_ref()&&n["click"]==true).unwrap();
            let b=&job["bounds"];let x=(b[0].as_f64().unwrap()+b[2].as_f64().unwrap())/2.0;let y=(b[1].as_f64().unwrap()+b[3].as_f64().unwrap())/2.0;
            let command=serde_json::json!([[0,x,y,y]]).to_string();
            app.azioni_accessibili(&command,3.0);
            assert_eq!(app.sopra,Some(Scheda::Lavori),"inset sottratto due volte");
        }
    }

    #[test]
    fn semantic_jobs_action_is_the_real_native_hit_target() {
        let mut app=App::new(ThemeId::Cedro);
        let mut cache=GlyphCache::new();
        let mut p=Painter::new(360,640,1.0,Fonts::for_theme(app.tema),&mut cache);
        p.accessibility_text=Some(Vec::new());
        app.disegna(&mut p,360.0,640.0);
        let data:serde_json::Value=serde_json::from_str(&app.accessibilita(&p)).unwrap();
        let job=data["nodes"].as_array().unwrap().iter().find(|n|n["text"]==cedro::i18n::tr("Lavori").as_ref()&&n["click"]==true).unwrap();
        let b=&job["bounds"];let x=((b[0].as_f64().unwrap()+b[2].as_f64().unwrap())/2.0) as f32;let y=((b[1].as_f64().unwrap()+b[3].as_f64().unwrap())/2.0) as f32;
        let command=serde_json::json!([[0,x*2.0,y*2.0,y*2.0]]).to_string();
        let effects=app.azioni_accessibili(&command,2.0);assert!(!effects.is_empty());
        assert_eq!(app.sopra,Some(Scheda::Lavori));assert!(app.casa_aperta);
    }
    #[test]
    fn review_regions_have_unique_ids_and_bridge_scroll_moves_content() {
        let mut app=App::new(ThemeId::Cedro);
        let rows=(0..8).map(|i|cedro::trascrizione::Segment{t0:i*100,t1:(i+1)*100,text:format!("Frase di prova {i}, leggibile con il suo timestamp."),speaker:None}).collect();
        let mut review=cedro::ui::review::Review::new(std::path::Path::new("/tmp/a11y-fixture.wav"),8.0,"it".into(),rows,vec![],None);review.view=2;
        app.apri_rivedi_con(review,"Prova".into(),0,None,None);
        let mut cache=GlyphCache::new();let mut p=Painter::new(360,800,1.0,Fonts::for_theme(app.tema),&mut cache);p.accessibility_text=Some(Vec::new());
        app.disegna(&mut p,360.0,800.0);
        let regions=app.colpi.iter().filter(|(_,az)|*az==Azione::Riv(rivedi::RivAz::Riga(0))).count();assert!(regions>=2,"fixture senza le due regioni reali");
        let data:serde_json::Value=serde_json::from_str(&app.accessibilita(&p)).unwrap();
        let mut ids=std::collections::HashSet::new();
        for node in data["nodes"].as_array().unwrap(){assert!(ids.insert(node["id"].as_u64().unwrap()),"id duplicato");}
        let before=app.scorri_rivedi_ora();
        let effects=app.azioni_accessibili("[[1,180,560,240]]",1.0);
        assert!(!effects.is_empty());assert!(app.scorri_rivedi_ora()>before,"il comando Java non scorre");
        assert!(app.azioni_accessibili("[[5,0,0,0]]",1.0).is_empty());
    }

    #[test]
    fn system_text_scale_changes_measured_layout() {
        let mut cache=GlyphCache::new();let mut p=Painter::new(360,640,1.0,Fonts::for_theme(ThemeId::Cedro),&mut cache);
        p.larger=false;let small=p.wrap_height("Documenti professionali e immagini",16.0,Weight::Regular,200.0,22.0,0);
        p.text_scale=2.0;let large=p.wrap_height("Documenti professionali e immagini",16.0,Weight::Regular,200.0,22.0,0);
        assert!(large>=small*2.0);
    }
}
