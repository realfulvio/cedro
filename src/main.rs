#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod cli;

#[cfg(windows)]
mod win;

#[cfg(windows)]
mod winfont;

#[cfg(windows)]
mod settings_win;
#[cfg(windows)]
mod conversioni_win;
#[cfg(windows)]
mod guida_win;
#[cfg(windows)]
mod progress_win;
#[cfg(windows)]
mod winlist;
#[cfg(windows)]
mod winmenu;
#[cfg(windows)]
mod wingfx;

#[cfg(target_os = "linux")]
mod linux_conversioni;
#[cfg(target_os = "linux")]
mod linux_trascrizione;
#[cfg(target_os = "linux")]
mod linux_youtube;
#[cfg(target_os = "linux")]
mod linux_progress;
#[cfg(target_os = "linux")]
mod linux_settings;
#[cfg(target_os = "linux")]
mod linux_ui;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(windows)]
    {
        if args.first().is_some_and(|a| a == "conversioni") {
            conversioni_win::run_standalone();
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
        unsafe {
            let _ = windows::Win32::System::Console::AllocConsole();
        }
        std::process::exit(cli::run());
    }
    #[cfg(target_os = "linux")]
    {
        let ui = args
            .first()
            .is_some_and(|arg| matches!(arg.as_str(), "show" | "settings" | "impostazioni" | "guida" | "conversioni" | "--background"));
        if args.is_empty() || ui {
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
