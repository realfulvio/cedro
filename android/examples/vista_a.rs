//! Rendering della UI reale A; i dati di prova restano soltanto in questo eseguibile host.
//! Eseguire sul mini-PC: cargo run -p cedro-android --release --example vista_a -- /tmp/vista-a
use std::path::{Path, PathBuf};
use std::time::Instant;
use cedro::{Kind, i18n::{self, Lang}};
use cedro::ui::{gfx::{Fonts, GlyphCache, Painter}, theme::ThemeId};
use cedro_android::ui::{App, Archivio, Lavoro, Recente, Scheda, Stato};

mod dicom_fixture {include!("../tests/support/dicom.rs");}

fn disegna(out: &Path, nome: &str, w: f32, h: f32, tema: ThemeId, lang: Lang, scene: usize) {
    let mut app = App::new(tema);
    app.anim.calmo = true;
    app.vai_casa();
    app.vista_completa(true);
    i18n::set(lang);
    app.lingua = lang.code();
    app.margini = (0.0, 27.0, 0.0, 14.0);
    let path = PathBuf::from("assets/cedro/immagini.jpg");
    app.recenti.push(Recente { path: path.clone(), nome: "Riunione.m4a".into(), kind: Kind::Audio, thumb: None });
    app.recenti.push(Recente { path: path.clone(), nome: "Relazione.pdf".into(), kind: Kind::Pdf, thumb: None });
    match scene {
        4 => {app.vista_nav(4);app.casa.link="https://archive.org/download/fixture/video.mp4".into();}
        1..=3|5..=6 => {
            let k = [Kind::Image, Kind::Pdf, Kind::Audio, Kind::Video, Kind::Other, Kind::Image][scene - 1];
            app.aggiungi_coda(vec![(path.clone(), if k == Kind::Image { "Sopralluogo.heic" } else { "Relazione.pdf" }.into(), k)]);
            app.info_voce(&path, cedro::ui::media::thumb_of(&path, 300), Some((4032, 3024)));
            app.vista_nav(scene);
            if scene == 1 { app.casa.operazione.insert(0, "pdf"); }

        }
        7 => { app.sopra = Some(Scheda::Lavori); app.lavori.push(Lavoro { id: 1, origine: "Sopralluogo.heic".into(), etichetta: "PDF".into(), conv: true, inizio: Instant::now(), quando: cedro_android::ui::adesso(), stato: Stato::Fatto { path: path.clone(), nome: "Sopralluogo.pdf".into(), msg: "1,2 MB".into() }, prog: Default::default(), sorgente: None, controlli: None }); }
        8 => app.sopra = Some(Scheda::Aspetto),
        9 => { app.imposta_file(Archivio { path: path.clone(), nome: "Sopralluogo.heic".into(), dim: 1234567 }, Kind::Image, true); app.miniatura = cedro::ui::media::thumb_of(&path, 300);
            app.foglio = false;
            app.dal_foglio = true;
            app.scelta = cedro::conversioni::formats(Kind::Image).iter().position(|f| f.id == "pdf");
        }
        10 => { app.grandi = true; }
        16 => { app.vista_nav(1);app.grandi=true;app.vista_scorri(750.0); }
        11..=15 => {
            app.vista_nav(6);
            let src=if scene==13 {
                let path=out.join("fixture-sintetica.dcm");dicom_fixture::crea(&path);path
            }else{PathBuf::from(if scene==12{"tests/fixtures/micro/jxr-rgb8.czi"}else{"tests/fixtures/svs/piramide.svs"})};
            app.aggiungi_coda(vec![(src.clone(),src.file_name().unwrap().to_string_lossy().into_owned(),Kind::Image)]);
            app.labs.caso=cedro::caso::Caso::new("V-2026-0457","Solo fixture del renderer");
            app.labs.path=Some(src.clone());
            app.labs.scelta=if scene==11{cedro_android::labs::Scelta{livello:Some(0),..Default::default()}}else{Default::default()};
            let temp=out.join(format!(".preview-{scene}"));
            match cedro_android::labs::anteprima(&src,&app.labs.scelta,&temp) {
                Ok((png,meta))=>{
                    app.labs.info=Some(serde_json::from_str(&meta).unwrap());
                    app.labs.immagine=cedro::ui::media::thumb_of(&png,1024);
                }
                Err(e)=>app.labs.errore=e,
            }
            let _=std::fs::remove_dir_all(temp);
            if scene==13 {app.vista_scorri(260.0);let _=std::fs::remove_file(&src);}
            if scene==14 {
                app.labs.codice="V-2026-0457".into();app.labs.nota="Nota fixture, esclusa dai nomi".into();app.labs.caso_aperto=true;
            }
            if scene==15 {app.grandi=true;app.vista_scorri(300.0);}
        }
        26..=32=>{
            use cedro::trascrizione::Segment;
            use cedro::ui::review::Review;
            let src=PathBuf::from("/fixture/Referto.wav");
            let rows=vec![
                Segment{t0:0,t1:800,text:"Tre mitosi in due punto cinque centimetri.".into(),speaker:Some(0)},
                Segment{t0:800,t1:1500,text:"Non si osserva atipia. La lesione è a sinistra.".into(),speaker:Some(1)},
                Segment{t0:1500,t1:2200,text:"Il campione è pronto per la revisione.".into(),speaker:Some(2)},
            ];
            let bars:Vec<_>=(0..600).map(|i|0.08+0.8*((i as f32*0.137).sin()*(i as f32*0.031).cos()).abs()).collect();
            if scene==31 {
                app.vista_nav(3);app.aggiungi_coda(vec![(src.clone(),"Riunione.wav".into(),Kind::Audio)]);
                app.onda_trascrizione(&src,Some(bars),243.3);app.referto=true;app.vista_scorri(190.0);
            }else if scene==32 {
                app.sopra=Some(Scheda::Lavori);
                app.lavori.push(Lavoro{id:4,origine:"Silenzio.wav".into(),etichetta:"Testo".into(),conv:false,inizio:Instant::now(),quando:cedro_android::ui::adesso(),stato:Stato::Fatto{path:PathBuf::new(),nome:String::new(),msg:format!("{} · 30,0 s. {}",i18n::tr("Nessun parlato riconosciuto"),i18n::tr("Prova una registrazione con la voce più vicina al microfono."))},prog:Default::default(),sorgente:None,controlli:None});
            }else{
                let mut r=Review::new(&src,243.3,"it".into(),rows.clone(),vec!["Voce uno".into(),"Voce due".into(),"Voce tre".into()],None);
                r.referto=scene==27;r.bars=Some(bars);r.detect();r.view=match scene{28=>2,29=>0,_=>1};
                app.apri_rivedi_con(r,"Referto di prova".into(),0,None,None);
                if scene==30 {app.frase_trascritta(7,src.clone(),None,243.3);for row in rows{app.frase_trascritta(7,src.clone(),Some(row),0.0);}}
            }
        }
        17..=25=>{
            app.vista_nav(4);
            app.casa.link=if scene<=20{"https://www.youtube.com/watch?v=fixture"}else{"https://example.org/video.mp4"}.into();
            app.web.altezza=720;
            app.web.subtitles=2;
            match scene {
                18|22=>app.vista_scorri(480.0),
                19|23=>app.vista_scorri(1180.0),
                20=>app.vista_scorri(1820.0),
                24|25=>{
                    app.sopra=Some(Scheda::Lavori);
                    app.lavori.push(Lavoro{id:2,origine:"Video pubblico di prova".into(),etichetta:"Video".into(),conv:false,inizio:Instant::now(),quando:cedro_android::ui::adesso(),stato:Stato::InCorso,prog:Default::default(),sorgente:None,controlli:None});
                    app.componente=Some(cedro_android::web::Componente{nome:"Python".into(),fase:if scene==24{cedro::componenti::Fase::Scarica{fatti:5_000_000,totale:14_000_000}}else{cedro::componenti::Fase::Errore}});
                    if scene==25{app.download_attesa=Some(cedro_android::web::AttesaDownload{nome:"Modello".into(),bytes:264_000_000,liberi:2_000_000_000,mobile:true,da:Instant::now()});}
                },
                _=>{},
            }
        }
        _ => {}
    }
    let mut cache = GlyphCache::new();
    let start = Instant::now();
    let mut p = Painter::new(w as u32, h as u32, 1.0, Fonts::for_theme(tema), &mut cache);
    p.larger = app.grandi;
    p.text_scale=std::env::var("CEDRO_RENDER_FONT_SCALE").ok().and_then(|s|s.parse::<f32>().ok()).filter(|s|s.is_finite()&&*s>0.0).unwrap_or(1.0);
    app.disegna(&mut p, w, h);
    let elapsed = start.elapsed().as_secs_f64() * 1000.0;
    p.save_png(&out.join(format!("{nome}.png"))).unwrap();
    println!("{nome}\t{elapsed:.2} ms");
}

fn main() {
    let out = PathBuf::from(std::env::args().nth(1).unwrap_or_else(|| "/tmp/vista-a".into()));
    std::fs::create_dir_all(&out).unwrap();
    let scenes = ["inizio", "converti", "pdf", "trascrivi", "web", "firma", "labs", "lavori", "impostazioni", "condividi", "grandi", "labs-livello", "labs-czi", "labs-dicom", "caso", "labs-grandi", "famiglie-grandi", "web-youtube", "web-youtube-qualita", "web-youtube-opzioni", "web-youtube-raccolte", "web-generico", "web-generico-qualita", "web-generico-opzioni", "componente-progresso", "componente-errore-rete", "onda-testo", "referto", "solo-testo", "solo-onda", "onda-live", "onda-prima", "nessun-parlato"];
    for lang in [Lang::It, Lang::De, Lang::Pt] {
        for (w, h) in [(360.0, 800.0), (360.0, 640.0), (800.0, 400.0)] {
            for (scene, name) in scenes.iter().enumerate() {
                disegna(&out, &format!("{}-{w}x{h}-{name}", lang.code()), w, h, ThemeId::Cedro, lang, scene);
            }
        }
    }
    for (name, theme) in [("notte", ThemeId::CedroNotte), ("bosco", ThemeId::CedroBosco), ("carta", ThemeId::CedroCarta)] {
        disegna(&out, name, 360.0, 800.0, theme, Lang::It, 0);
    }
}
