use cedro::{perform, read_quality, Kind, Mode};

pub fn run() -> i32 {
    let args: Vec<String> = std::env::args().skip(1).collect();
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
                quality = v.parse::<u8>().unwrap_or(quality).clamp(40, 95);
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
    let cmd = positional[0].as_str();
    let result = match cmd {
        "convert" | "tool" if positional.len() >= 3 => {
            perform(std::path::Path::new(&positional[1]), &positional[2], quality)
        }
        "update" => return update(),
        "trascrivi" if positional.len() >= 2 => return transcribe(&positional[1..], &args),
        "zip" if positional.len() >= 2 => perform(std::path::Path::new(&positional[1]), "zip", quality),
        "slices" if positional.len() >= 2 => {
            let path = std::path::Path::new(&positional[1]);
            let mode = match positional.get(2).map(String::as_str) {
                Some("tools") => Mode::Tools,
                Some("actions") => Mode::Actions,
                _ => Mode::Convert,
            };
            let kind = if path.exists() { cedro::kind_of(path) } else { Kind::Other };
            for slice in cedro::slices(kind, mode) {
                println!("{}\t{}", slice.id, slice.label);
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
            println!("{}\n{}", out.message, out.path.display());
            0
        }
        Err(e) => {
            eprintln!("{e}");
            1
        }
    }
}

fn update() -> i32 {
    use cedro::update;
    let release = match update::check() {
        Ok(Some(release)) => release,
        Ok(None) => {
            println!("Cedro {} è già l'ultima versione.", update::CURRENT);
            return 0;
        }
        Err(e) => {
            eprintln!("{e}");
            return 1;
        }
    };
    println!("C'è Cedro {} (hai la {}).\n{}", release.version, update::CURRENT, release.page);
    print!("Vuoi aggiornare? [s/N] ");
    let _ = std::io::Write::flush(&mut std::io::stdout());
    let mut answer = String::new();
    let _ = std::io::stdin().read_line(&mut answer);
    if !matches!(answer.trim().to_lowercase().as_str(), "s" | "si" | "sì" | "y" | "yes") {
        return 0;
    }
    match install(&release) {
        Ok(note) => {
            println!("{note}");
            0
        }
        Err(e) => {
            eprintln!("{e}");
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
        return Ok(format!("Avviato {}.", asset.name));
    }
    // Su Linux il pacchetto si scarica e si installa sempre da solo, da dove gira Cedro (anche da `target/`
    // o da una cartella a scelta): la copia nuova va in ~/.local/opt/cedro. Senza pacchetto si dice chiaro.
    #[cfg(unix)]
    {
        let asset = release.package.as_ref().ok_or_else(|| {
            format!("la release {} non ha il pacchetto per questo sistema (linux-{}): riprova più tardi", release.version, cedro::update::package_arch())
        })?;
        let installed = cedro::update::install_package(asset)?;
        return Ok(format!("Installata la versione {} in {}.", release.version, installed.display()));
    }
    #[cfg(not(unix))]
    {
        std::process::Command::new("explorer").arg(&release.page).spawn().map_err(|e| e.to_string())?;
        Ok(format!("Aperta la pagina della release: {}", release.page))
    }
}

fn print_help() {
    eprintln!(
        "\
Cedro — conversione locale, senza upload.

  cedro convert <file> <png|jpeg|webp|bmp|gif|tiff|pdf|txt|md|csv|wav|mp3|mp4|docx|jpg>
  cedro tool <file> <compress|half|gray|rot90|strip|upper|lower|ocr|dup|path|zip>
  cedro zip <file>
  cedro --quality 80 convert foto.png jpeg
  cedro update     controlla se su GitHub c'è una versione nuova
  cedro trascrivi <audio>… [--testo] [--precisione veloce|buona|massima] [--cpu]
                   dalla voce a un Word (o a un testo), con il motore scaricato alla prima volta

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
L'originale non viene mai sovrascritto."
    );
}

/// `cedro trascrivi`: la stessa trascrizione della scheda, con i passi stampati man mano.
fn transcribe(files: &[String], args: &[String]) -> i32 {
    use cedro::trascrizione::{self, Event, Format, Request};
    let model = match args.iter().position(|a| a == "--precisione").and_then(|i| args.get(i + 1)).map(String::as_str) {
        Some("veloce") => 0,
        Some("massima") => 2,
        Some("buona") => 1,
        _ => trascrizione::default_model(),
    };
    let settings = cedro::read_settings();
    let request = Request {
        format: if args.iter().any(|a| a == "--testo") { Format::Text } else { Format::Word },
        model,
        gpu: settings.gpu && !args.iter().any(|a| a == "--cpu"),
        avx512: settings.avx512,
    };
    let paths: Vec<std::path::PathBuf> = files.iter().filter(|f| !f.starts_with("--") && !matches!(f.as_str(), "veloce" | "buona" | "massima")).map(std::path::PathBuf::from).collect();
    let failed = std::sync::atomic::AtomicBool::new(false);
    trascrizione::run(&paths, &request, None, &cedro::Options::default(), &|event| match event {
        Event::Log(line) => println!("{} {}", line.key, line.text),
        Event::Percent { .. } => {}
        Event::Finished { path, result } => match result {
            Ok((out, msg)) => println!("OK {} -> {} ({msg})", path.display(), out.display()),
            Err(e) => {
                failed.store(true, std::sync::atomic::Ordering::Relaxed);
                eprintln!("ERRORE {}: {e}", path.display());
            }
        },
    });
    i32::from(failed.load(std::sync::atomic::Ordering::Relaxed))
}
