#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod cli;

#[cfg(windows)]
mod win;

#[cfg(windows)]
mod winfont;

#[cfg(windows)]
mod settings_win;
#[cfg(windows)]
mod guida_win;
#[cfg(windows)]
mod vista_win;
#[cfg(windows)]
mod progress_win;
#[cfg(windows)]
mod winlist;
#[cfg(windows)]
mod winmenu;
#[cfg(windows)]
mod wingfx;
#[cfg(windows)]
mod firma_win;
#[cfg(windows)]
mod ui_win;
#[cfg(windows)]
mod aspetto_win;

#[cfg(target_os = "linux")]
mod linux_firma;
#[cfg(target_os = "linux")]
mod linux_font;
#[cfg(target_os = "linux")]
mod linux_progress;
#[cfg(target_os = "linux")]
mod linux_settings;
#[cfg(target_os = "linux")]
mod linux_ui;
#[cfg(target_os = "linux")]
mod linux_ui2;

fn main() {
    // `args_os`: un nome di file non UTF-8 (Linux) con `args` farebbe chiudere Cedro prima di aprire la finestra.
    let args: Vec<String> = std::env::args_os().skip(1).map(|a| a.to_string_lossy().into_owned()).collect();
    // La lingua si sceglie subito: anche le finestre che partono da sole (guida, vista) la usano.
    let _ = cedro::read_settings();
    #[cfg(windows)]
    {
        if args.first().is_some_and(|a| a == "conversioni") {
            ui_win::run_standalone();
            return;
        }
        if let Some(path) = viewer_arg(&args) {
            ui_win::run_viewer(path);
            return;
        }
        // Dal disinstallatore, senza console: via le associazioni dei file firmati.
        if args.first().is_some_and(|a| a == "associa-no") {
            let _ = cedro::associa::apply(false, false);
            return;
        }
        if args.first().is_some_and(|a| a == "vista") {
            vista_win::run_standalone();
            return;
        }
        if args.first().is_some_and(|a| a == "guida") {
            guida_win::run_standalone();
            return;
        }
        if args.first().is_some_and(|a| a == "impostazioni" || a == "settings") {
            settings_win::run_standalone();
            return;
        }
        if args.is_empty() {
            if let Err(err) = win::start() {
                cedro::append_log(&err);
            }
            return;
        }
        // Da cmd o PowerShell si scrive nella loro console; una console nuova si chiuderebbe subito, senza farsi leggere.
        unsafe {
            use windows::Win32::System::Console::{AllocConsole, AttachConsole, ATTACH_PARENT_PROCESS};
            if AttachConsole(ATTACH_PARENT_PROCESS).is_err() {
                let _ = AllocConsole();
            }
        }
        std::process::exit(cli::run());
    }
    #[cfg(target_os = "linux")]
    {
        let ui = viewer_arg(&args).is_some()
            || args
                .first()
                .is_some_and(|arg| matches!(arg.as_str(), "show" | "settings" | "impostazioni" | "guida" | "vista" | "conversioni" | "--background"));
        if args.is_empty() || ui {
            // Cedro ha i suoi colori chiari: un tema scuro imposto da fuori (GTK_THEME=Breeze:dark,
            // Adwaita:dark…) dipingerebbe di nero le parti che il suo CSS non copre.
            std::env::remove_var("GTK_THEME");
            std::process::exit(linux_ui::start());
        }
        std::process::exit(cli::run());
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        if args.is_empty() {
            eprintln!("Senza argomenti la ruota parte su Windows e su Linux.");
        }
        std::process::exit(cli::run());
    }
}

/// `cedro verifica <file>` (il doppio clic su un file firmato) o un solo file firmato: il visualizzatore.
#[cfg(any(windows, target_os = "linux"))]
fn viewer_arg(args: &[String]) -> Option<std::path::PathBuf> {
    match args {
        [cmd, path] if cmd == "verifica" => Some(path.into()),
        [path] if cedro::firma::is_signed_name(path) && std::path::Path::new(path).is_file() => Some(path.into()),
        _ => None,
    }
}
