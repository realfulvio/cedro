use cedro::{perform, read_quality, Kind, Mode};
use std::ffi::OsString;

pub fn run() -> i32 {
    // I percorsi restano `OsString`: un nome di file non UTF-8 (Linux) con `args` farebbe chiudere Cedro.
    let args: Vec<OsString> = std::env::args_os().skip(1).collect();
    let _ = cedro::read_settings();
    if args.is_empty() || args.iter().any(|a| a == "-h" || a == "--help") {
        print_help();
        return if args.is_empty() { 2 } else { 0 };
    }
    let mut quality = read_quality();
    let mut positional = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--quality" {
            if let Some(v) = args.get(i + 1) {
                quality = v.to_str().and_then(|v| v.parse::<u8>().ok()).unwrap_or(quality).clamp(40, 95);
                i += 2;
                continue;
            }
        }
        positional.push(args[i].clone());
        i += 1;
    }
    if positional.is_empty() {
        print_help();
        return 2;
    }
    let cmd = positional[0].to_string_lossy();
    let result = match cmd.as_ref() {
        "convert" | "tool" if positional.len() >= 3 => {
            perform(std::path::Path::new(&positional[1]), &positional[2].to_string_lossy(), quality)
        }
        "update" => return update(),
        // Dal disinstallatore: Cedro toglie le sue associazioni dei file firmati.
        "associa-no" => {
            return match cedro::associa::apply(false, false) {
                Ok(()) => 0,
                Err(e) => {
                    eprintln!("{e}");
                    1
                }
            }
        }
        // La verifica di un file firmato, scritta nel terminale.
        "firma" if positional.len() >= 2 => {
            let mut bad = false;
            for p in &positional[1..] {
                let r = cedro::firma::verify_path(std::path::Path::new(p), cedro::firma::Opts::default());
                bad |= r.level == cedro::firma::Level::Bad;
                println!("{}", cedro::firma::rapporto::text(&[&r]));
            }
            return if bad { 1 } else { 0 };
        }
        "trascrivi" if positional.len() >= 2 => return transcribe(&positional[1..], &args),
        // Il pacchetto dei dati (BagIt con le impronte SHA-256): file e cartelle in uno ZIP.
        "pacchetto" if positional.len() >= 2 => {
            let files: Vec<std::path::PathBuf> = positional[1..].iter().map(std::path::PathBuf::from).collect();
            cedro::package_paths(&files, None)
        }
        // La figura per un articolo: i file nell'ordine dato diventano i pannelli A, B, C.
        "figura" if positional.len() >= 2 => {
            let mut figura = cedro::figura::Opzioni::default();
            let mut files = Vec::new();
            for a in &positional[1..] {
                match a.to_str() {
                    Some("--colonna") => figura.larghezza_mm = 85,
                    Some("--600") => figura.dpi = 600,
                    Some("--senza-lettere") => figura.lettere = false,
                    Some("--senza-barra") => figura.barra = false,
                    // --obiettivo 40: la barra sulle foto senza misure, dalla calibrazione del 10x.
                    Some(o) if o.starts_with("--obiettivo=") => {
                        figura.obiettivo = o["--obiettivo=".len()..].trim_end_matches(['x', 'X']).parse().unwrap_or(0);
                        figura.calibrazione_10x = cedro::read_settings().calibrazione_10x;
                    }
                    _ => files.push(std::path::PathBuf::from(a)),
                }
            }
            let exists = if cedro::read_settings().replace { cedro::Exists::Replace } else { cedro::Exists::Number };
            cedro::figure_with(&files, None, &cedro::Options { figura, exists, ..cedro::Options::default() })
        }
        "zip" if positional.len() >= 2 => perform(std::path::Path::new(&positional[1]), "zip", quality),
        "slices" if positional.len() >= 2 => {
            let path = std::path::Path::new(&positional[1]);
            let mode = match positional.get(2).and_then(|m| m.to_str()) {
                Some("tools") => Mode::Tools,
                Some("actions") => Mode::Actions,
                _ => Mode::Convert,
            };
            let kind = if path.exists() { cedro::kind_of(path) } else { Kind::Other };
            for slice in cedro::slices(kind, mode) {
                println!("{}\t{}", slice.id, cedro::i18n::tr(slice.label));
            }
            return 0;
        }
        _ => {
            print_help();
            return 2;
        }
    };
    match result {
        Ok(out) => {
            println!("{}\n{}", cedro::i18n::tr(&out.message), out.path.display());
            0
        }
        Err(e) => {
            eprintln!("{}", cedro::i18n::tr(&e.to_string()));
            1
        }
    }
}

fn update() -> i32 {
    use cedro::update;
    let release = match update::check() {
        Ok(Some(release)) => release,
        Ok(None) => {
            println!("{}", cedro::i18n::tr(&format!("Cedro {} è già l'ultima versione.", update::CURRENT)));
            return 0;
        }
        Err(e) => {
            eprintln!("{}", cedro::i18n::tr(&e));
            return 1;
        }
    };
    println!("{}", cedro::i18n::tr(&format!("C'è Cedro {} (hai la {}).\n{}", release.version, update::CURRENT, release.page)));
    print!("{}", cedro::i18n::tr("Vuoi aggiornare? [s/N] "));
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let mut answer = String::new();
    let _ = std::io::stdin().read_line(&mut answer);
    if !matches!(answer.trim().to_lowercase().as_str(), "s" | "si" | "sì" | "y" | "yes") {
        return 0;
    }
    match install(&release) {
        Ok(note) => {
            println!("{}", cedro::i18n::tr(&note));
            0
        }
        Err(e) => {
            eprintln!("{}", cedro::i18n::tr(&e));
            1
        }
    }
}

/// Su Windows scarica e avvia l'installatore; su Linux installa il pacchetto se può. Altrimenti apre la pagina della release.
pub fn install(release: &cedro::update::Release) -> Result<String, String> {
    #[cfg(windows)]
    if let Some(asset) = &release.installer {
        let path = cedro::update::download(asset)?;
        std::process::Command::new(&path).spawn().map_err(|e| e.to_string())?;
        return Ok(cedro::i18n::tr_string(&format!("Avviato {}.", asset.name)));
    }
    // Su Linux il pacchetto si scarica e si installa sempre da solo, da dove gira Cedro (anche da `target/`
    // o da una cartella a scelta): la copia nuova va in ~/.local/opt/cedro. Senza pacchetto si dice chiaro.
    #[cfg(unix)]
    {
        let asset = release.package.as_ref().ok_or_else(|| {
            cedro::i18n::tr_string(&format!("la release {} non ha il pacchetto per questo sistema (linux-{}): riprova più tardi", release.version, cedro::update::package_arch()))
        })?;
        let installed = cedro::update::install_package(asset)?;
        Ok(cedro::i18n::tr_string(&format!("Installata la versione {} in {}.", release.version, installed.display())))
    }
    #[cfg(not(unix))]
    {
        std::process::Command::new("explorer").arg(&release.page).spawn().map_err(|e| e.to_string())?;
        Ok(cedro::i18n::tr_string(&format!("Aperta la pagina della release: {}", release.page)))
    }
}

/// Aggiornamento silenzioso (Windows): scarica l'installatore e lo lancia senza finestre, a priorità bassa.
/// L'installatore chiude Cedro e lo riapre da solo; il Cedro nuovo avvisa che è tutto a posto.
#[cfg(windows)]
pub fn install_silent(release: &cedro::update::Release) -> Result<(), String> {
    use std::os::windows::process::CommandExt;
    let asset = release.installer.as_ref().ok_or("la release non ha l'installatore per questo sistema")?;
    let path = cedro::update::download(asset)?;
    cedro::update::mark_updated(&release.version);
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x0000_4000;
    let started = std::process::Command::new(&path)
        .args(["/VERYSILENT", "/SUPPRESSMSGBOXES", "/NORESTART", "/SP-", "/SILENTUPDATE"])
        .creation_flags(CREATE_NO_WINDOW | BELOW_NORMAL_PRIORITY_CLASS)
        .spawn();
    if let Err(e) = started {
        let _ = std::fs::remove_file(cedro::config_dir().join("updated.txt"));
        return Err(e.to_string());
    }
    Ok(())
}

fn print_help() {
    let help = "\
Cedro — conversione locale, senza upload.

  cedro convert <file> <png|jpeg|webp|avif|jxl|bmp|gif|tiff|ico|tga|qoi|pnm|hdr|exr|pdf|txt|md|csv|wav|mp3|flac|opus|aiff|mp4|docx|jpg|z-webp|z-jpeg|z-png|z-mp3|z-wav|z-pdf>
  cedro tool <file> <compress|half|gray|rot90|strip|upper|lower|ocr|dup|path|zip>
  cedro zip <file>
  cedro --quality 80 convert foto.png jpeg
  cedro update     controlla se su GitHub c'è una versione nuova
  cedro trascrivi <audio>… [--testo] [--precisione veloce|buona|massima] [--voci auto|1-8] [--cpu]
                   dalla voce a un Word (o a un testo), con il motore scaricato alla prima volta;
                   --voci dice quante persone parlano (auto le conta da solo, 1 non le riconosce)
  cedro figura <immagine>… [--colonna] [--600] [--senza-lettere] [--senza-barra] [--obiettivo=40x]
                   la figura per un articolo: pannelli A, B, C in un TIFF a 300 dpi largo 17,5 cm
  cedro pacchetto <file o cartella>…   pacchetto di dati (ZIP BagIt con l'impronta SHA-256 di ogni file)
  cedro tool <pacchetto.zip> verify-pack   controlla che nessun file del pacchetto sia cambiato
  cedro firma <file>…   verifica le firme digitali (.p7m, .p7s, .m7m, .tsd, .tsr, PDF firmati)
  cedro verifica <file> apre il visualizzatore dei file firmati (come il doppio clic)

Senza argomenti, su Windows e su Linux, Cedro resta nella tray.
Su Linux la ruota si apre dal menu del file, oppure così:
  cedro show --conversione <file>
  cedro show --strumenti <file>
  cedro show --azioni <file>
La prima volta scrive i menu di Dolphin e di Nautilus.
Su Linux la configurazione sta in ~/.config/cedro/config.txt.

Le conversioni sono nel programma: non servono LibreOffice, ffmpeg, Tesseract né Poppler.
Il video MP4, su Windows, usa i codificatori di Windows (H.264 e AAC).
Su Linux usa OpenH264 e FDK AAC, compilati dentro il programma.
L'MP3 usa LAME, compilato dentro l'eseguibile.
L'originale non viene mai sovrascritto.";
    let help: Vec<String> = help.lines().map(cedro::i18n::tr_string).collect();
    eprintln!("{}", help.join("\n"));
}

/// `cedro trascrivi`: la stessa trascrizione della scheda, con i passi stampati man mano.
fn transcribe(files: &[OsString], args: &[OsString]) -> i32 {
    use cedro::trascrizione::{self, Event, Format, Request};
    let model = match args.iter().position(|a| a == "--precisione").and_then(|i| args.get(i + 1)).and_then(|a| a.to_str()) {
        Some("veloce") => 0,
        Some("massima") => 2,
        Some("buona") => 1,
        _ => trascrizione::default_model(),
    };
    let voices = args.iter().position(|a| a == "--voci").and_then(|i| args.get(i + 1)).and_then(|a| a.to_str());
    let speakers = match voices {
        None | Some("auto") => 0,
        Some(n) => match n.parse::<u8>() {
            Ok(n) if (1..=trascrizione::MAX_VOICES as u8).contains(&n) => n,
            _ => {
                eprintln!("{}", cedro::i18n::tr("--voci vuole auto o un numero da 1 a 8"));
                return 2;
            }
        },
    };
    let settings = cedro::read_settings();
    let request = Request {
        format: if args.iter().any(|a| a == "--testo") { Format::Text } else { Format::Word },
        model,
        gpu: settings.gpu && !args.iter().any(|a| a == "--cpu"),
        avx512: settings.avx512,
        speakers,
    };
    let paths: Vec<std::path::PathBuf> = files
        .iter()
        .filter(|f| !f.as_encoded_bytes().starts_with(b"--") && !matches!(f.to_str(), Some("veloce" | "buona" | "massima")) && Some(f.as_os_str()) != voices.map(std::ffi::OsStr::new))
        .map(std::path::PathBuf::from)
        .collect();
    let failed = std::sync::atomic::AtomicBool::new(false);
    trascrizione::run(&paths, &request, None, &cedro::Options::default(), &|event| match event {
        Event::Log(line) => match line.speaker {
            Some(v) => println!("{} {}: {}", line.key, trascrizione::speaker_name(v as usize), line.text),
            None => println!("{} {}", line.key, cedro::i18n::tr(&line.text)),
        },
        Event::Percent { .. } => {}
        Event::Finished { path, result } => match result {
            Ok((out, msg)) => println!("OK {} -> {} ({})", path.display(), out.display(), cedro::i18n::tr(&msg)),
            Err(e) => {
                failed.store(true, std::sync::atomic::Ordering::Relaxed);
                eprintln!("{} {}: {}", cedro::i18n::tr("ERRORE"), path.display(), cedro::i18n::tr(&e));
            }
        },
    });
    i32::from(failed.load(std::sync::atomic::Ordering::Relaxed))
}
