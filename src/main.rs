#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod cli;

#[cfg(windows)]
mod win;

#[cfg(windows)]
mod winfont;

#[cfg(windows)]
mod settings_win;

#[cfg(target_os = "linux")]
mod linux_ui;

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    #[cfg(windows)]
    {
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
        let show = args.first().is_some_and(|arg| arg == "show");
        if args.is_empty() || show {
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
