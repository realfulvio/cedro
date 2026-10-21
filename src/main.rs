#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod cli;

#[cfg(windows)]
mod win;

fn main() {
    if std::env::args().len() > 1 {
        #[cfg(windows)]
        unsafe {
            let _ = windows::Win32::System::Console::AllocConsole();
        }
        std::process::exit(cli::run());
    }
    #[cfg(windows)]
    {
        if let Err(err) = win::start() {
            cedro::append_log(&err);
        }
        return;
    }
    #[cfg(not(windows))]
    {
        eprintln!("Senza argomenti la ruota parte solo su Windows.");
        cli::run();
    }
}
