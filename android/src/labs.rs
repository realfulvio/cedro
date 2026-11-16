//! Adattamento del laboratorio al telefono: metadati e anteprima della sorgente.
//! La decodifica gira nel processo lavoro, con gli stessi lettori del desktop.
use std::path::{Path, PathBuf};
use serde::{Deserialize, Serialize};
use cedro::conversioni::{self, OptionDef};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Scelta {
    /// None = panoramica; Some = area centrale del livello, senza decodificare il pieno.
    pub livello: Option<usize>,
    pub scena: usize,
    pub canale: Option<usize>,
    pub z: usize,
    pub t: usize,
    pub frame: usize,
    pub finestra: u32,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Info {
    pub livelli: Vec<(u32, u32)>,
    pub scene: Vec<String>,
    pub canali: Vec<String>,
    pub z: usize,
    pub t: usize,
    pub frames: usize,
    pub descrizione: String,
    pub avviso: String,
}

pub fn estensione(path: &Path) -> String {
    path.extension().and_then(|s| s.to_str()).unwrap_or("").to_ascii_lowercase()
}
pub fn vetrino(path: &Path) -> bool { matches!(estensione(path).as_str(), "svs" | "ndpi" | "mrxs") }
pub fn micro(path: &Path) -> bool { matches!(estensione(path).as_str(), "czi" | "nd2" | "lif") }
pub fn dicom(path: &Path) -> bool { matches!(estensione(path).as_str(), "dcm" | "dicom") }
pub fn laboratorio(path: &Path) -> bool { vetrino(path) || micro(path) || dicom(path) }
pub fn foglio(path: &Path) -> bool { matches!(estensione(path).as_str(), "xlsx" | "xls" | "ods") }

/// Le definizioni restano quelle del motore. Nessuna apertura di file nel loop UI.
/// Le modalità per assi singoli restano valide (il motore le riduce a un solo piano).
pub fn defs<'a>(kind: cedro::Kind, action: &str, paths: impl Iterator<Item = &'a Path>) -> Vec<&'static OptionDef> {
    let paths: Vec<_> = paths.collect();
    let mut out = Vec::new();
    if paths.iter().any(|p| vetrino(p)) { out.extend(conversioni::slide_option_defs(action)); }
    if paths.iter().any(|p| dicom(p)) { out.extend(conversioni::dicom_option_defs(action)); }
    if paths.iter().any(|p| micro(p)) { out.extend(conversioni::micro_option_defs(action, 2, 2, 2)); }
    if paths.iter().any(|p| foglio(p)) { out.extend(conversioni::sheet_option_defs(action)); }
    let sheet=action=="pdf" && paths.iter().any(|p|foglio(p));
    for d in conversioni::option_defs(kind, action) {
        if sheet && d.opt==conversioni::Opt::PageSize {continue;}
        if !out.iter().any(|o| o.opt == d.opt) { out.push(d); }
    }
    out
}

/// Salva solo un'anteprima temporanea. Il chiamante la rimuove, non la pubblica.
pub fn anteprima(path: &Path, scelta: &Scelta, out: &Path) -> Result<(PathBuf, String), String> {
    std::fs::create_dir_all(out).map_err(|e| e.to_string())?;
    let png = out.join("anteprima.png");
    let mut info = Info::default();
    if vetrino(path) {
        let s = cedro::svs::Slide::open(path)?;
        info.livelli = s.levels.iter().map(|l| (l.width, l.height)).collect();
        info.descrizione = format!("{} · {} × {}",estensione(path).to_uppercase(),s.full().width,s.full().height);
        let img = if let Some(k) = scelta.livello {
            let l = s.levels.get(k).ok_or("Livello non presente nel file")?;
            let (w,h) = (l.width.min(1024),l.height.min(1024));
            info.avviso = "Anteprima: area centrale del livello. L’esportazione usa le opzioni del risultato.".into();
            s.read_region(path,k,(l.width-w)/2,(l.height-h)/2,w,h)?
        } else { s.preview(path, 1024)? };
        img.save(&png).map_err(|e| e.to_string())?;
    } else if dicom(path) {
        use cedro::dicom::pixel as px;
        let f = cedro::dicom::DicomFile::open(path)?;
        let i = px::Info::of(&f)?;
        info.frames = i.frames as usize;
        // Nessun identificativo del paziente nell'intestazione dell'anteprima.
        info.descrizione = format!("DICOM · {} × {} · {}", i.cols, i.rows, i.frames);
        info.avviso = "Eventuali nomi impressi nei pixel richiedono una verifica visiva.".into();
        let frame = px::frame(&f,&i,scelta.frame)?;
        let window = match scelta.finestra { 1 => px::LUNG, 2 => px::BONE, 3 => px::SOFT, _ => px::Window::File };
        px::render(&i,&frame,window).thumbnail(1024,1024).save(&png).map_err(|e|e.to_string())?;
    } else if micro(path) {
        let p = cedro::microscopia::Pila::open_serie(path,scelta.scena)?;
        info.scene = p.serie.clone();
        info.canali = p.canali.iter().map(|c| c.nome.clone()).collect();
        info.z = p.z;
        info.t = p.t;
        info.descrizione = format!("{} · {} × {}",estensione(path).to_uppercase(),p.larghezza,p.altezza);
        info.avviso = p.avviso.clone().unwrap_or_default();
        if scelta.z >= p.z || scelta.t >= p.t { return Err("Piano non presente nel file".into()); }
        if let Some(c) = scelta.canale {
            if c >= p.canali.len() { return Err("Canale non presente nel file".into()); }
            let piano = p.piano_z(path,c,scelta.z,scelta.t)?;
            cedro::microscopia::grigio_di(&p,&piano,true).thumbnail(1024,1024).save(&png).map_err(|e|e.to_string())?;
        } else {
            let canali = p.canali_di_z(path,scelta.z,scelta.t)?;
            // Il compositore condiviso conserva i colori definiti nel file.
            let rgb = cedro::microscopia::composita(&p,&canali,true);
            image::imageops::thumbnail(&rgb,1024,1024).save(&png).map_err(|e|e.to_string())?;
        }
    } else { return Err("Il file non contiene un’anteprima di laboratorio".into()); }
    Ok((png,serde_json::to_string(&info).map_err(|e|e.to_string())?))
}

#[cfg(test)]
mod tests {
    use super::*;
    mod dicom_fixture {include!("../tests/support/dicom.rs");}
    #[test]
    fn opzioni_contestuali_ufficio_e_laboratorio() {
        use cedro::{Kind, conversioni::Opt};
        for (path,action,kind,required) in [
            ("a.docx","pdf",Kind::Doc,vec![Opt::OfficePage]),
            ("a.xls","pdf",Kind::Text,vec![Opt::OfficePage,Opt::SheetFit,Opt::SheetScope]),
            ("a.ndpi","png",Kind::Image,vec![Opt::SlideLevel,Opt::SlideLabel]),
            ("a.dcm","png",Kind::Image,vec![Opt::DicomWindow,Opt::DicomAnon,Opt::Dicom16]),
            ("a.czi","tiff",Kind::Image,vec![Opt::MicroChannels,Opt::MicroZ,Opt::Micro16]),
        ] {
            let defs = defs(kind,action,std::iter::once(Path::new(path)));
            for opt in required { assert!(defs.iter().any(|d|d.opt==opt),"{path}: {opt:?}"); }
            for (i,d) in defs.iter().enumerate() { assert!(!defs[i+1..].iter().any(|e|e.opt==d.opt)); }
        }
        let s=Scelta{scena:2,canale:Some(1),z:3,t:4,..Default::default()};
        assert_eq!(serde_json::from_str::<Scelta>(&serde_json::to_string(&s).unwrap()).unwrap(),s);
    }
    #[test]
    fn anteprima_fixture_svs_czi_e_livello_reale() {
        let dir=crate::storage::nuova_cartella(&std::env::temp_dir()).unwrap();
        let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/fixtures");
        for (file,scelta) in [
            ("svs/piramide.svs",Scelta::default()),
            ("svs/piramide.svs",Scelta{livello:Some(0),..Default::default()}),
            ("micro/jxr-rgb8.czi",Scelta{canale:Some(0),..Default::default()}),
        ] {
            let (png,json)=anteprima(&root.join(file),&scelta,&dir).unwrap();
            let info:Info=serde_json::from_str(&json).unwrap();
            let img=image::open(png).unwrap();
            assert!(img.width()<=1024 && img.height()<=1024);
            assert!(!info.descrizione.is_empty());
            if file.ends_with("svs"){assert!(!info.livelli.is_empty());}else{assert!(!info.canali.is_empty());}
        }
        assert!(anteprima(&root.join("svs/piramide.svs"),&Scelta{livello:Some(usize::MAX),..Default::default()},&dir).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn anteprima_dicom_cambia_frame_e_finestra() {
        let dir=crate::storage::nuova_cartella(&std::env::temp_dir()).unwrap();
        let path=dir.join("fixture.dcm");dicom_fixture::crea(&path);
        use cedro::dicom::pixel as px;
        let f=cedro::dicom::DicomFile::open(&path).unwrap();
        let info=px::Info::of(&f).unwrap();
        assert_eq!(info.modality,"CT");assert_eq!(info.intercept,-1024.0);
        let frame=px::frame(&f,&info,0).unwrap();
        let px::Frame::Gray(values)=&frame else{panic!("fixture non monocromatica")};
        let hu:Vec<_>=values.iter().map(|v|*v as f64-1024.0).collect();
        assert_eq!(px::effective_window(&info,&hu,px::Window::File),(40.0,400.0));
        assert_eq!(px::effective_window(&info,&hu,px::BONE),(400.0,1800.0));
        assert_eq!(px::effective_window(&info,&hu,px::Window::Auto),(992.0,4032.0));
        let (png,meta)=anteprima(&path,&Scelta::default(),&dir).unwrap();
        assert_eq!(serde_json::from_str::<Info>(&meta).unwrap().frames,2);
        let first=image::open(&png).unwrap().to_rgba8();
        anteprima(&path,&Scelta{frame:1,..Default::default()},&dir).unwrap();
        let second=image::open(&png).unwrap().to_rgba8();
        assert!(first.as_raw()!=second.as_raw(),"cambiare frame deve cambiare i pixel");
        assert_eq!(first.get_pixel(512,128).0,[255,255,255,255]);
        assert_eq!(second.get_pixel(512,128).0,[0,0,0,255]);
        anteprima(&path,&Scelta{finestra:2,..Default::default()},&dir).unwrap();
        let bone=image::open(&png).unwrap().to_rgba8();
        let expected=px::render(&info,&frame,px::BONE).thumbnail(1024,1024).to_rgba8();
        let auto=px::render(&info,&frame,px::Window::Auto).thumbnail(1024,1024).to_rgba8();
        assert!(bone.as_raw()==expected.as_raw(),"la scelta Osso deve applicare il preset del core");
        assert!(first.as_raw()!=bone.as_raw(),"file e Osso devono produrre pixel diversi");
        assert!(first.as_raw()!=auto.as_raw()&&bone.as_raw()!=auto.as_raw(),"né file né Osso devono cadere sull'auto-window");
        assert!(anteprima(&path,&Scelta{frame:99,..Default::default()},&dir).is_err());
        std::fs::remove_dir_all(dir).unwrap();
    }

}
