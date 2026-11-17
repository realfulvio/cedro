//! Protocollo del processo di conversione. Nessun JNI nel figlio; la pubblicazione resta al genitore.
use std::path::PathBuf;
use serde::{Deserialize, Serialize};
static CANCEL:std::sync::atomic::AtomicBool=std::sync::atomic::AtomicBool::new(false);

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Voce {pub modello:usize,pub voci:u8,#[serde(default)]pub referto:bool}
impl Voce {
    pub fn da_codice(code:usize)->Self{Self{modello:code&0x0f,voci:(code>>4) as u8,referto:false}}
}

#[derive(Serialize, Deserialize)]
pub struct Richiesta {
    pub file: PathBuf,
    pub azione: String,
    pub qualita: u8,
    pub voce: Voce,
    pub out: PathBuf,
    pub opts: cedro::Options,
    pub unisci: Vec<(PathBuf, Option<cedro::PageSel>)>,
    pub sommario: Option<(Vec<String>,Vec<String>)>,
    #[serde(default)]
    pub anteprima: Option<crate::labs::Scelta>,
    #[serde(default)]
    pub web:Option<crate::web::Scelte>,
}

#[derive(Serialize, Deserialize)]
pub enum Evento {
    Audio(PathBuf,f64),
    Frase(PathBuf,cedro::trascrizione::Segment),
    Parziale(crate::web::Parziale),
    Scarica(String,u64),
    Componente(Option<crate::web::Componente>),
    Progresso(u32),
    Passo(String, String),
    Fine(Result<(PathBuf, String), String>),
}

fn emetti(e: Evento) {
    use std::io::Write;
    #[cfg(target_os="android")]
    {
        use std::os::fd::FromRawFd;
        static PIPE: std::sync::OnceLock<std::sync::Mutex<std::fs::File>> = std::sync::OnceLock::new();
        let pipe=PIPE.get_or_init(|| {
            let fd=std::env::var("CEDRO_JOB_FD").ok().and_then(|v|v.parse().ok()).expect("canale del launcher");
            std::sync::Mutex::new(unsafe {std::fs::File::from_raw_fd(fd)})
        });
        let mut out=pipe.lock().unwrap_or_else(|e|e.into_inner());
        let _=serde_json::to_writer(&mut *out,&e);
        let _=writeln!(out);
        let _=out.flush();
    }
    #[cfg(not(target_os="android"))]
    { let _=writeln!(std::io::sink(),"{}",serde_json::to_string(&e).unwrap_or_default()); }
}

#[cfg(target_os="android")]
fn consenso(evento:Evento)->Result<(),String>{
    use std::os::fd::FromRawFd;
    use std::io::BufRead;
    static PERMESSO:std::sync::Mutex<()>=std::sync::Mutex::new(());
    let _lock=PERMESSO.lock().unwrap_or_else(|e|e.into_inner());
    let fd:i32=std::env::var("CEDRO_JOB_FD").map_err(|e|e.to_string())?.parse::<i32>().map_err(|e|e.to_string())?;
    let dup=unsafe{libc::dup(fd)};
    if dup<0{return Err("Canale del motore non disponibile".into());}
    let file=unsafe{std::fs::File::from_raw_fd(dup)};
    emetti(evento);
    let mut line=String::new();
    std::io::BufReader::new(file).read_line(&mut line).map_err(|e|e.to_string())?;
    serde_json::from_str::<Result<(),String>>(&line).map_err(|e|e.to_string())?
}

/// Chiamato soltanto dal launcher PIE nell'APK. Un panic non attraversa l'ABI C.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn cedro_lavoro_main() -> i32 {
    extern "C" fn annulla(_:i32){CANCEL.store(true,std::sync::atomic::Ordering::Release);}
    unsafe{libc::signal(libc::SIGUSR1,annulla as libc::sighandler_t);}
    cedro::componenti::osserva(Some(std::sync::Arc::new(|s|emetti(Evento::Componente(s.map(crate::web::Componente::da_core))))));
    cedro::componenti::verifica_download(Some(std::sync::Arc::new(|nome,bytes|consenso(Evento::Scarica(nome.into(),bytes)))));
    cedro::componenti::traccia_parziali(Some(std::sync::Arc::new(|path|consenso(Evento::Parziale(crate::web::Parziale::registra(path)?)))));
    let r = std::panic::catch_unwind(|| {
        use std::io::Read;
        let request: Richiesta = serde_json::from_reader(std::io::stdin().take(4 * 1024 * 1024)).map_err(|e| e.to_string())?;
        if let Ok(libs)=std::env::var("CEDRO_LIB_DIR") {cedro::riassunto::android::set_libs(PathBuf::from(libs));}
        esegui(request)
    }).unwrap_or_else(|_| Err("Il motore si è interrotto".into()));
    let code = if r.is_ok() { 0 } else { 1 };
    emetti(Evento::Fine(r));
    code
}

/// Ha un lifetime indipendente dal journal dei risultati completi.
struct Scratch(PathBuf);
impl Drop for Scratch {fn drop(&mut self){let _=std::fs::remove_dir_all(&self.0);}}

/// Prepara l'audio effettivo per trascrizione e revisione, senza applicare due volte il nome del caso.
fn prepara_audio_web(file:&std::path::Path,out:&std::path::Path,scelte:&crate::web::Scelte,opts:&cedro::Options)->Result<PathBuf,String>{
    let dir=out.join(".revisione");
    std::fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
    let mut audio=opts.clone();audio.name_template=None;audio.caso=None;
    audio.exists=cedro::Exists::Replace;audio.on_exists=None;
    audio.normalize=scelte.normalize;audio.mp3_kbps=None;
    let quality=[95,82,55][scelte.audio_quality.min(2) as usize];
    let prepared=cedro::perform_with(file,&scelte.audio_format,quality,Some(&dir),&audio).map_err(|e|e.to_string())?.path;
    crate::storage::atomico(&out.join(".revisione-source"),&serde_json::to_vec(&prepared).map_err(|e|e.to_string())?).map_err(|e|e.to_string())?;
    Ok(prepared)
}

fn converti_web(r:&mut Richiesta,testo:bool,audio:bool){
    // Qualità media e selezione vocale sono campi separati; non alterare qualita.
    r.azione=if testo{"trascrivi"}else if audio{"mp3"}else{"mp4"}.into();
}

fn esegui(mut r: Richiesta) -> Result<(PathBuf, String), String> {
    let _scratch=Scratch(r.out.join(".scaricato"));
    if r.azione=="strip" && crate::labs::micro(&r.file) {
        return Err("La copia anonima di CZI, ND2 e LIF non è supportata dal motore. Puoi convertire il file in PNG; non è una copia anonima del formato originale.".into());
    }
    if let Some(scelta) = r.anteprima.take() { return crate::labs::anteprima(&r.file,&scelta,&r.out); }
    if let Some((lines,names))=r.sommario.take() {
        use cedro::riassunto::{Failure, Options, Progress, Step};
        let done=std::sync::Mutex::new(None);
        cedro::riassunto::run(lines,names,Options{gpu:false,avx512:false},&CANCEL,&|p|match p {
            Progress::Step(s)=>emetti(Evento::Passo("Riassunto".into(),match s {
                Step::Waiting=>"Aspetto che finisca l'altro lavoro…".into(),
                Step::Download(t)=>format!("Modello: {t}"),
                Step::Loading=>"Carico il modello in memoria…".into(),
                Step::Reading(_,1)=>"Legge la riunione…".into(),
                Step::Reading(i,n)=>format!("Legge la riunione: parte {} di {n}",i+1),
                Step::Merging=>"Mette insieme gli appunti…".into(),
                Step::Writing=>"Scrive il riassunto…".into(),
            })),
            Progress::Percent(n)=>emetti(Evento::Progresso(n as u32*10)),
            Progress::Done(r)=>*done.lock().unwrap_or_else(|e|e.into_inner())=Some(r),
            _=>{},
        });
        let result=done.into_inner().unwrap_or_else(|e|e.into_inner()).ok_or("Il riassunto non è partito")?;
        let stored=result.map_err(|e|match e {Failure::Cancelled=>"Annullato".into(),Failure::Other(e)=>e,Failure::LowMemory(n)=>format!("Poca memoria libera ({})",cedro::riassunto::gb(n))})?;
        let path=r.out.join("riassunto.json");
        crate::storage::atomico(&path,stored.to_json().to_string().as_bytes()).map_err(|e|e.to_string())?;
        return Ok((path,String::new()));
    }
    if r.azione.starts_with("web-") {
        use cedro::webvideo::{Want,resolve,download};
        let text=r.azione=="web-testo";
        let scelte=r.web.clone().unwrap_or_default();
        let want=match r.azione.as_str(){"web-audio"|"web-testo"=>Want::Audio,"web-leggero" if scelte.altezza>0=>Want::Light,_=>Want::Video};
        let media=resolve(&r.file.to_string_lossy(),want)?;
        r.qualita=if want==Want::Audio {[95,82,55][scelte.audio_quality.min(2) as usize]}else{scelte.media_quality};
        r.voce=scelte.voce;
        r.opts.video_height=(scelte.altezza>0).then_some(scelte.altezza);
        r.opts.normalize=scelte.normalize;
        let convert=!text&&(want==Want::Audio||scelte.altezza>0);
        let download_dir=r.out.join(".scaricato");
        let file=download(&media,&download_dir,&|f|emetti(Evento::Progresso((f*600.0) as u32)),&CANCEL)?;
        if !text&&!convert{return crate::web::nomina(&file,&r.out,&r.opts).map(|p|(p,media.title));}
        r.file=if text{prepara_audio_web(&file,&r.out,&scelte,&r.opts)?}else{file};
        converti_web(&mut r,text,want==Want::Audio);
        if !text&&want==Want::Audio {r.azione=scelte.audio_format;}
    }
    if r.azione == "trascrivi" {
        if r.file.starts_with(r.out.join(".revisione")){
            let data=serde_json::to_vec(&r.file).map_err(|e|e.to_string())?;
            crate::storage::atomico(&r.out.join(".revisione-source"),&data).map_err(|e|e.to_string())?;
        }
        use cedro::trascrizione::{Event, Format, Request};
        let req = Request { format: Format::Text, model: r.voce.modello.min(cedro::trascrizione::MODELS.len() - 1), gpu: false, avx512: false, speakers: r.voce.voci, glossario: r.voce.referto.then(|| "veterinaria".into()) };
        let result = std::sync::Mutex::new(None);
        let duration = std::cell::Cell::new(0.0);
        cedro::trascrizione::run_cancellable(&[r.file.clone()], &req, Some(&r.out), &r.opts, &CANCEL, &|e| match e {
            Event::Audio { secs } => {duration.set(secs);emetti(Evento::Audio(r.file.clone(),secs));},
            Event::Segment(s) => emetti(Evento::Frase(r.file.clone(),s)),
            Event::Percent { percent, .. } => emetti(Evento::Progresso(percent as u32 * 10)),
            Event::Log(l) => emetti(Evento::Passo(l.key, l.text)),
            Event::Finished { result: r, .. } => *result.lock().unwrap_or_else(|e| e.into_inner()) = Some(r),
        });
        let result=result.into_inner().unwrap_or_else(|e| e.into_inner()).unwrap_or_else(|| Err("La trascrizione non è partita".into()));
        if matches!(&result,Err(e) if e=="nessun parlato riconosciuto nell'audio") {
            let _=std::fs::remove_file(cedro::revisione::review_path(&r.file));
            return Ok((PathBuf::new(),format!("{} · {:.1} s. {}",cedro::i18n::tr("Nessun parlato riconosciuto"),duration.get(),cedro::i18n::tr("Prova una registrazione con la voce più vicina al microfono."))));
        }
        return result;
    }
    cedro::progress::con(|f| emetti(Evento::Progresso((f * 1000.0) as u32)), || {
        if r.azione == "zip-tutti" {
            let files: Vec<PathBuf> = r.unisci.iter().map(|u| u.0.clone()).collect();
            cedro::zip_paths(&files, &r.out, None, &r.opts)
        } else if matches!(r.azione.as_str(),"figure"|"rx"|"package") {
            let mut files:Vec<_>=if r.unisci.is_empty(){vec![r.file.clone()]}else{r.unisci.iter().map(|u|u.0.clone()).collect()};
            if r.azione=="package" {
                let cartelle:Vec<_>=files.iter().filter(|p|crate::labs::estensione(p)=="mrxs").map(|p|p.with_extension("")).collect();
                for dir in cartelle {if !files.contains(&dir){files.push(dir);}}
            }
            match r.azione.as_str(){
                "figure"=>cedro::figure_with(&files,Some(&r.out),&r.opts),
                "rx"=>cedro::rx_with(&files,Some(&r.out),&r.opts),
                _=>cedro::package_with(&files,Some(&r.out),&r.opts),
            }
        } else if r.unisci.is_empty() {
            cedro::perform_with(&r.file, &r.azione, r.qualita, Some(&r.out), &r.opts)
        } else {
            cedro::merge_with(&r.unisci, Some(&r.out), &r.opts)
        }
    }).map(|o| (o.path, o.message)).map_err(|e| e.to_string())
}

#[cfg(target_os = "android")]
pub fn avvia(libs: &std::path::Path, r: Richiesta, cancel: &std::sync::atomic::AtomicBool, inputs: Vec<std::sync::Arc<std::fs::File>>, permesso:impl Fn(&str,u64)->Result<(),String>, evento: impl Fn(Evento)) -> Result<(PathBuf, String), String> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::sync::atomic::Ordering;
    // Serialize before starting: an unsupported option cannot leave a child waiting on stdin.
    let data = serde_json::to_vec(&r).map_err(|e| e.to_string())?;
    let mut parziali=Vec::new();
    if data.len()>4*1024*1024 {return Err("Troppe opzioni per il lavoro".into());}
    if cancel.load(Ordering::Acquire) { return Err("Annullato".into()); }
    use std::os::fd::AsRawFd;
    let (events,write_events)=std::os::unix::net::UnixStream::pair().map_err(|e|e.to_string())?;
    let job_fd=write_events.as_raw_fd();
    let mut fds: Vec<i32> = inputs.iter().map(|f|f.as_raw_fd()).collect();
    fds.push(job_fd);
    let mut command = Command::new(libs.join("libcedro_lavoro.so"));
    command
        .arg(libs.join("libcedro_android.so")).env("LD_LIBRARY_PATH", libs).env("CEDRO_JOB_FD",job_fd.to_string()).env("CEDRO_LIB_DIR",libs)
        .process_group(0).stdin(Stdio::piped()).stdout(Stdio::null()).stderr(Stdio::null());
    unsafe { command.pre_exec(move || {
        for fd in &fds {
            if libc::fcntl(*fd,libc::F_SETFD,0)<0 { return Err(std::io::Error::last_os_error()); }
        }
        Ok(())
    }); }
    let child = command.spawn().map_err(|e| format!("Il motore non parte: {e}"))?;
    struct Guardia(std::process::Child);
    impl Drop for Guardia {
        fn drop(&mut self) {
            // Kill the whole group (also external decoders), then reap the direct child.
            unsafe { libc::kill(-(self.0.id() as i32), libc::SIGKILL); }
            let _ = self.0.kill();
            let _ = self.0.wait();
        }
    }
    let mut child = Guardia(child);
    let mut stdin = child.0.stdin.take().ok_or("Canale del motore non disponibile")?;
    stdin.write_all(&data).map_err(|e| e.to_string())?;
    drop(stdin);
    drop(write_events);
    let mut risposte=events.try_clone().map_err(|e|e.to_string())?;
    let stdout = events;
    let (tx, rx) = std::sync::mpsc::sync_channel(32);
    let reader = std::thread::spawn(move || {
        for line in BufReader::new(stdout).lines() {
            let Ok(line) = line else { break };
            if let Ok(e) = serde_json::from_str::<Evento>(&line) {
                if tx.send(e).is_err() { break; }
            }
        }
    });
    let mut annullamento=None;
    let result = loop {
        if cancel.load(Ordering::Acquire) {
            let since=annullamento.get_or_insert_with(||{
                unsafe{libc::kill(child.0.id() as i32,libc::SIGUSR1);}
                std::time::Instant::now()
            });
            if since.elapsed()>std::time::Duration::from_millis(1500){break Err("Annullato".into());}
        }
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(Evento::Parziale(file))=>{
                let path=&file.path;
                let risposta:Result<(),String>=if path.starts_with(cedro::trascrizione::data_dir())&&!path.components().any(|c|matches!(c,std::path::Component::ParentDir))&&path.extension().is_some_and(|e|e=="part"){
                    parziali.push(file);Ok(())
                }else{Err("Percorso del componente non valido".into())};
                if serde_json::to_writer(&mut risposte,&risposta).is_err()||writeln!(risposte).is_err(){break Err("Canale del motore non disponibile".into());}
            },
            Ok(Evento::Scarica(nome,bytes))=>{
                let risposta=permesso(&nome,bytes);
                if serde_json::to_writer(&mut risposte,&risposta).is_err()||writeln!(risposte).is_err(){break Err("Canale del motore non disponibile".into());}
            },
            Ok(Evento::Fine(r)) => break if cancel.load(Ordering::Acquire){Err("Annullato".into())}else{r},
            Ok(e) => evento(e),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {},
            Err(_) => break Err("Il motore si è interrotto".into()),
        }
    };
    drop(rx);
    drop(child); // Even on success, no background engine survives the job.
    let _ = reader.join();
    if cancel.load(Ordering::Acquire){crate::web::rimuovi_parziali(&parziali);return Err("Annullato".into());}
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn copia_anonima_micro_non_legge_il_file_e_pacchetto_contiene_i_dati() {
        let dir=crate::storage::nuova_cartella(&std::env::temp_dir()).unwrap();
        for ext in ["czi","nd2","lif"] {
            let r=Richiesta{file:dir.join(format!("inesistente.{ext}")),azione:"strip".into(),qualita:82,voce:Default::default(),out:dir.clone(),opts:Default::default(),unisci:vec![],sommario:None,anteprima:None,web:None};
            assert!(esegui(r).unwrap_err().contains("non è supportata"));
        }
        let a=dir.join("a.txt");let b=dir.join("b.txt");
        std::fs::write(&a,b"a").unwrap();std::fs::write(&b,b"b").unwrap();
        let out=dir.join("uscite");std::fs::create_dir_all(&out).unwrap();
        let r=Richiesta{file:a.clone(),azione:"package".into(),qualita:82,voce:Default::default(),out,opts:cedro::caso::opzioni(Some("C-1"),Default::default()),unisci:vec![(a,None),(b,None)],sommario:None,anteprima:None,web:None};
        let (zip,_)=esegui(r).unwrap();
        assert!(zip.file_name().unwrap().to_string_lossy().contains("C-1"));
        let check=cedro::pacchetto::verifica(&zip,&|_|{}).unwrap();
        assert!(check.tutto_bene());assert_eq!(check.integri,2);
        assert!(zip.with_file_name(format!("{}.sha256",zip.file_name().unwrap().to_string_lossy())).is_file());
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn audio_web_testo_preparato_rispetta_formato_qualita_volume(){
        let root=crate::storage::nuova_cartella(&std::env::temp_dir()).unwrap();
        let downloaded=root.join(".scaricato");std::fs::create_dir(&downloaded).unwrap();
        let scratch=Scratch(downloaded.clone());let source=downloaded.join("originale.wav");
        // Due secondi PCM mono a basso volume, senza codec o fixture esterni.
        let data:Vec<u8>=(0..88200).flat_map(|i|(((i as f32*0.063).sin()*2000.0) as i16).to_le_bytes()).collect();
        let mut wav=b"RIFF".to_vec();wav.extend_from_slice(&(36+data.len() as u32).to_le_bytes());wav.extend_from_slice(b"WAVEfmt ");
        wav.extend_from_slice(&16u32.to_le_bytes());wav.extend_from_slice(&1u16.to_le_bytes());wav.extend_from_slice(&1u16.to_le_bytes());
        wav.extend_from_slice(&44100u32.to_le_bytes());wav.extend_from_slice(&88200u32.to_le_bytes());wav.extend_from_slice(&2u16.to_le_bytes());wav.extend_from_slice(&16u16.to_le_bytes());
        wav.extend_from_slice(b"data");wav.extend_from_slice(&(data.len() as u32).to_le_bytes());wav.extend_from_slice(&data);std::fs::write(&source,wav).unwrap();
        let opts=cedro::caso::opzioni(Some("C-42"),cedro::Options{name_template:Some("%caso%_%nome%".into()),..Default::default()});
        let mut lengths=Vec::new();
        for (i,format,quality,normalize) in [(0,"wav",0,false),(1,"wav",0,true),(2,"mp3",0,false),(3,"mp3",2,false)]{
            let out=root.join(i.to_string());let choices=crate::web::Scelte{audio_format:format.into(),audio_quality:quality,normalize,..Default::default()};
            let saved=prepara_audio_web(&source,&out,&choices,&opts).unwrap();
            assert_eq!(saved,out.join(".revisione").join(format!("originale.{format}")));
            let marker:PathBuf=serde_json::from_slice(&std::fs::read(out.join(".revisione-source")).unwrap()).unwrap();assert_eq!(marker,saved);
            let bytes=std::fs::read(&saved).unwrap();
            if format=="wav"{
                assert_eq!(&bytes[..4],b"RIFF");assert_eq!(&bytes[8..12],b"WAVE");
                let mut at=12;let mut peak=0i32;
                while at+8<=bytes.len(){
                    let len=u32::from_le_bytes(bytes[at+4..at+8].try_into().unwrap()) as usize;
                    if &bytes[at..at+4]==b"data"{peak=bytes[at+8..at+8+len].chunks_exact(2).map(|v|(i16::from_le_bytes([v[0],v[1]]) as i32).abs()).max().unwrap();break;}
                    at+=8+len+(len%2);
                }
                assert!((if normalize{31990..=32010}else{1990..=2010}).contains(&peak),"picco PCM: {peak}");
            }else{
                // Decodifica col core: l'estensione da sola non prova che sia un MP3 valido.
                let decoded=cedro::perform_with(&saved,"wav",82,Some(&out.join("verifica")),&Default::default()).unwrap();
                assert_eq!(&std::fs::read(decoded.path).unwrap()[..4],b"RIFF");lengths.push(bytes.len());
            }
            let transcript=out.join("originale.txt");std::fs::write(&transcript,b"testo").unwrap();
            assert_eq!(crate::web::nomina(&transcript,&out,&opts).unwrap().file_name().unwrap(),"C-42_originale.txt");
        }
        assert!(lengths[0]>lengths[1],"qualità massima/leggera: {lengths:?}");
        assert_eq!(opts.caso.as_deref(),Some("C-42"));
        drop(scratch);assert!(!downloaded.exists());
        for (i,format) in [(0,"wav"),(1,"wav"),(2,"mp3"),(3,"mp3")]{assert!(root.join(i.to_string()).join(".revisione").join(format!("originale.{format}")).is_file());}
        std::fs::remove_dir_all(root).unwrap();
    }
    #[test]
    fn qualita_web_indipendente_da_modello_e_voci(){
        for code in [0,1,2,16,33,66]{
            for (text,audio,action) in [(false,true,"mp3"),(false,false,"mp4"),(true,true,"trascrivi")]{
                let mut r=Richiesta{file:"link".into(),azione:"web-audio".into(),qualita:82,voce:Voce::da_codice(code),out:"out".into(),opts:Default::default(),unisci:vec![],sommario:None,anteprima:None,web:None};
                converti_web(&mut r,text,audio);
                let r:Richiesta=serde_json::from_slice(&serde_json::to_vec(&r).unwrap()).unwrap();
                assert_eq!(r.qualita,82);assert_eq!(r.voce,Voce::da_codice(code));assert_eq!(r.azione,action);
            }
        }
    }
    #[test]
    fn opzioni_worker_senza_perdite() {
        let mut o = cedro::Options::default();
        o.password = Some("segreto\nprova".into());
        o.slide_level = 2;
        o.micro_all_t = true;
        o.pdf_pages.plan = cedro::Plan::EveryPage;
        o.ufficio.solo_attivo = true;
        o.tessere.lato = 512;
        let data = serde_json::to_vec(&o).unwrap();
        assert_eq!(serde_json::from_slice::<cedro::Options>(&data).unwrap(), o);
    }
}
