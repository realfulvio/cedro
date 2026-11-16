//! Protocollo del processo di conversione. Nessun JNI nel figlio; la pubblicazione resta al genitore.
use std::path::PathBuf;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Voce {pub modello:usize,pub voci:u8}
impl Voce {
    pub fn da_codice(code:usize)->Self{Self{modello:code&0x0f,voci:(code>>4) as u8}}
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
}

#[derive(Serialize, Deserialize)]
pub enum Evento {
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

/// Chiamato soltanto dal launcher PIE nell'APK. Un panic non attraversa l'ABI C.
#[cfg(target_os = "android")]
#[no_mangle]
pub extern "C" fn cedro_lavoro_main() -> i32 {
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

pub fn sposta_download(file:&std::path::Path,out:&std::path::Path,revisione:bool)->Result<PathBuf,String>{
    let dir=if revisione{out.join(".revisione")}else{out.to_path_buf()};
    std::fs::create_dir_all(&dir).map_err(|e|e.to_string())?;
    let target=dir.join(file.file_name().ok_or("Nome del risultato assente")?);
    std::fs::rename(file,&target).map_err(|e|e.to_string())?;
    Ok(target)
}

fn converti_web(r:&mut Richiesta,testo:bool,audio:bool){
    // Qualità media e selezione vocale sono campi separati; non alterare qualita.
    r.azione=if testo{"trascrivi"}else if audio{"mp3"}else{"mp4"}.into();
}

fn esegui(mut r: Richiesta) -> Result<(PathBuf, String), String> {
    let _scratch=Scratch(r.out.join(".scaricato"));
    if let Some((lines,names))=r.sommario.take() {
        use cedro::riassunto::{Failure, Options, Progress, Step};
        let done=std::sync::Mutex::new(None);
        cedro::riassunto::run(lines,names,Options{gpu:false,avx512:false},&std::sync::atomic::AtomicBool::new(false),&|p|match p {
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
        let want=match r.azione.as_str(){"web-audio"|"web-testo"=>Want::Audio,"web-leggero"=>Want::Light,_=>Want::Video};
        let media=resolve(&r.file.to_string_lossy(),want)?;
        let convert=!text&&((want==Want::Audio&&!media.audio)||(want==Want::Light&&media.size.is_none_or(|n|n>30_000_000)&&!media.url.contains("512kb")));
        let download_dir=r.out.join(".scaricato");
        let file=download(&media,&download_dir,&|f|emetti(Evento::Progresso((f*600.0) as u32)),&std::sync::atomic::AtomicBool::new(false))?;
        if !text&&!convert{return sposta_download(&file,&r.out,false).map(|p|(p,media.title));}
        r.file=if text{sposta_download(&file,&r.out,true)?}else{file};
        converti_web(&mut r,text,want==Want::Audio);
    }
    if r.azione == "trascrivi" {
        if r.file.starts_with(r.out.join(".revisione")){
            let data=serde_json::to_vec(&r.file).map_err(|e|e.to_string())?;
            crate::storage::atomico(&r.out.join(".revisione-source"),&data).map_err(|e|e.to_string())?;
        }
        use cedro::trascrizione::{Event, Format, Request};
        let req = Request { format: Format::Text, model: r.voce.modello.min(cedro::trascrizione::MODELS.len() - 1), gpu: false, avx512: false, speakers: r.voce.voci, glossario: None };
        let result = std::sync::Mutex::new(None);
        cedro::trascrizione::run(&[r.file.clone()], &req, Some(&r.out), &r.opts, &|e| match e {
            Event::Percent { percent, .. } => emetti(Evento::Progresso(percent as u32 * 10)),
            Event::Log(l) => emetti(Evento::Passo(l.key, l.text)),
            Event::Finished { result: r, .. } => *result.lock().unwrap_or_else(|e| e.into_inner()) = Some(r),
        });
        let result=result.into_inner().unwrap_or_else(|e| e.into_inner()).unwrap_or_else(|| Err("La trascrizione non è partita".into()));
        return result;
    }
    cedro::progress::con(|f| emetti(Evento::Progresso((f * 1000.0) as u32)), || {
        if r.azione == "zip-tutti" {
            let files: Vec<PathBuf> = r.unisci.iter().map(|u| u.0.clone()).collect();
            cedro::zip_paths(&files, &r.out, None, &r.opts)
        } else if r.unisci.is_empty() {
            cedro::perform_with(&r.file, &r.azione, r.qualita, Some(&r.out), &r.opts)
        } else {
            cedro::merge_with(&r.unisci, Some(&r.out), &r.opts)
        }
    }).map(|o| (o.path, o.message)).map_err(|e| e.to_string())
}

#[cfg(target_os = "android")]
pub fn avvia(libs: &std::path::Path, r: Richiesta, cancel: &std::sync::atomic::AtomicBool, inputs: Vec<std::sync::Arc<std::fs::File>>, evento: impl Fn(Evento)) -> Result<(PathBuf, String), String> {
    use std::io::{BufRead, BufReader, Write};
    use std::os::unix::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::sync::atomic::Ordering;
    // Serialize before starting: an unsupported option cannot leave a child waiting on stdin.
    let data = serde_json::to_vec(&r).map_err(|e| e.to_string())?;
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
    let result = loop {
        if cancel.load(Ordering::Acquire) { break Err("Annullato".into()); }
        match rx.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(Evento::Fine(r)) => break r,
            Ok(e) => evento(e),
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {},
            Err(_) => break Err("Il motore si è interrotto".into()),
        }
    };
    drop(rx);
    drop(child); // Even on success, no background engine survives the job.
    let _ = reader.join();
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn qualita_web_indipendente_da_modello_e_voci(){
        for code in [0,1,2,16,33,66]{
            for (text,audio,action) in [(false,true,"mp3"),(false,false,"mp4"),(true,true,"trascrivi")]{
                let mut r=Richiesta{file:"link".into(),azione:"web-audio".into(),qualita:82,voce:Voce::da_codice(code),out:"out".into(),opts:Default::default(),unisci:vec![],sommario:None};
                converti_web(&mut r,text,audio);
                let r:Richiesta=serde_json::from_slice(&serde_json::to_vec(&r).unwrap()).unwrap();
                assert_eq!(r.qualita,82);assert_eq!(r.voce,Voce::da_codice(code));assert_eq!(r.azione,action);
            }
        }
    }
    #[test]
    fn scratch_scompare_ma_audio_revisione_resta_riapribile(){
        for revision in [false,true]{
            let root=crate::storage::nuova_cartella(&std::env::temp_dir()).unwrap();
            let downloaded=root.join(".scaricato");std::fs::create_dir(&downloaded).unwrap();
            let source=downloaded.join("audio.wav");std::fs::write(&source,b"audio originale").unwrap();
            let saved={let _guard=Scratch(downloaded.clone());if revision{Some(sposta_download(&source,&root,true).unwrap())}else{std::fs::write(root.join("converted.mp3"),b"converted").unwrap();None}};
            assert!(!downloaded.exists());
            if let Some(saved)=saved{
                let encoded=serde_json::to_vec(&saved).unwrap();
                let restarted:PathBuf=serde_json::from_slice(&encoded).unwrap();
                assert_eq!(std::fs::read(restarted).unwrap(),b"audio originale");
            }else{assert_eq!(std::fs::read_dir(&root).unwrap().count(),1);}
            std::fs::remove_dir_all(root).unwrap();
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
