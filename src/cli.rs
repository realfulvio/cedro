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

fn print_help() {
    eprintln!(
        "\
Cedro — conversione locale, senza upload.

  cedro convert <file> <png|jpeg|webp|bmp|gif|tiff|pdf|txt|md|csv|wav|mp3|mp4>
  cedro tool <file> <compress|half|gray|rot90|strip|upper|lower|dup|path|zip>
  cedro zip <file>
  cedro --quality 80 convert foto.png jpeg

Senza argomenti, su Windows parte la ruota nella tray.
Audio e video richiedono ffmpeg già presente nel PATH.
L'originale non viene mai sovrascritto."
    );
}
