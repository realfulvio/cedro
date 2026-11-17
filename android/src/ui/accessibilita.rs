//! Semantica della UI Rust, derivata dagli stessi bersagli realmente attivabili.
use super::*;
use std::hash::{Hash,Hasher};
impl App {
    pub fn accessibilita(&self,p:&Painter)->String {
        let mut nodes=Vec::new();
        let texts=p.accessibility_text.as_deref().unwrap_or(&[]);
        let mut used=std::collections::HashSet::new();
        for (shape,action) in &self.colpi {
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
            let mut hash=std::collections::hash_map::DefaultHasher::new();format!("{action:?}").hash(&mut hash);
            let id=(hash.finish()&0x3fffffff) as u32+1;
            nodes.push(serde_json::json!({"id":id,"text":label,"bounds":bounds,"click":true,"selected":selected,"checkable":checkable,"checked":selected}));
        }
        for (i,(text,bounds)) in texts.iter().enumerate().filter(|(i,_)|!used.contains(i)) {
            nodes.push(serde_json::json!({"id":0x40000000u32+i as u32,"text":text,"bounds":bounds,"click":false}));
        }
        serde_json::json!({"nodes":nodes,"scroll":self.scorri_max>0.0||self.rivedi.is_some()}).to_string()
    }
}
