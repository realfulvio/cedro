//! Il codice aperto che Cedro usa, per la scheda «Informazioni»: chi l'ha scritto, con che licenza, a cosa serve.
//! L'elenco completo (tutte le librerie, con i testi delle licenze) lo fa `cargo run --example licenze`.

/// Un componente di terzi.
pub struct Component {
    pub name: &'static str,
    pub author: &'static str,
    /// Come si legge sul sito del progetto, per esempio «MIT o Apache-2.0».
    pub license: &'static str,
    /// A cosa serve in Cedro.
    pub what: &'static str,
    pub url: &'static str,
    /// Funzione ancora in lavorazione: la voce c'è già, con l'etichetta «in arrivo».
    pub soon: bool,
    /// Si usa solo su Linux.
    pub linux_only: bool,
}

const fn c(name: &'static str, author: &'static str, license: &'static str, what: &'static str, url: &'static str) -> Component {
    Component { name, author, license, what, url, soon: false, linux_only: false }
}

#[allow(dead_code)]
const fn soon(name: &'static str, author: &'static str, license: &'static str, what: &'static str, url: &'static str) -> Component {
    Component { name, author, license, what, url, soon: true, linux_only: false }
}

const fn linux(name: &'static str, author: &'static str, license: &'static str, what: &'static str, url: &'static str) -> Component {
    Component { name, author, license, what, url, soon: false, linux_only: true }
}

pub struct Group {
    pub title: &'static str,
    pub items: &'static [Component],
}

pub const GROUPS: &[Group] = &[
    Group {
        title: "Documenti e PDF",
        items: &[
            c("hayro", "Laurenz Stampfl", "Apache-2.0 o MIT", "Disegna le pagine dei PDF.", "github.com/LaurenzV/hayro"),
            c("lopdf", "Junfeng Liu", "MIT", "Legge e scrive i file PDF.", "github.com/J-F-Liu/lopdf"),
            c("skrifa", "Google Fonts", "MIT o Apache-2.0", "Legge i caratteri dentro i PDF.", "github.com/googlefonts/fontations"),
            c("cfb", "Matthew D. Steele", "MIT", "Apre i vecchi file Word .doc.", "github.com/mdsteele/rust-cfb"),
            c("zip", "Mathijs van de Nes e altri", "MIT", "Archivi ZIP e file .docx.", "github.com/zip-rs/zip2"),
        ],
    },
    Group {
        title: "Immagini e OCR",
        items: &[
            c("image, png", "The image-rs Developers", "MIT o Apache-2.0", "JPG, PNG, WebP, GIF, BMP, TIFF, ICO, TGA, QOI, PNM, HDR, OpenEXR, DDS.", "github.com/image-rs/image"),
            c("resvg", "Yevhenii Reizner e collaboratori", "Apache-2.0 o MIT", "Disegna le immagini SVG.", "github.com/linebender/resvg"),
            c("ravif, rav1e, avif-decode, rav1d", "Kornel Lesiński, Xiph.Org, Memory Safety Initiative", "BSD-3-Clause o BSD-2-Clause", "Legge e scrive le foto AVIF.", "github.com/kornelski/cavif-rs"),
            c("jxl-oxide, zune-jpegxl", "Wonwoo Choi, Caleb Etemesi", "MIT o Apache-2.0", "Legge e scrive le foto JPEG XL.", "github.com/tirr-c/jxl-oxide"),
            c("ocrs, rten", "Robert Knight", "MIT o Apache-2.0", "Riconoscimento del testo e i suoi calcoli.", "github.com/robertknight/ocrs"),
            c("Modelli OCR di ocrs", "Robert Knight", "CC BY-SA 4.0", "I modelli che leggono il testo nelle immagini.", "github.com/robertknight/ocrs-models"),
        ],
    },
    Group {
        title: "Audio e video",
        items: &[
            c("LAME", "The LAME Project", "LGPL-3.0", "Audio MP3.", "lame.sourceforge.io"),
            c("Symphonia", "Philip Deljanov", "MPL-2.0", "Legge MP3, FLAC, OGG, AAC, WAV, AIFF.", "github.com/pdeljanov/Symphonia"),
            c("oxideav VP8 e VP9", "Mark Karpeles", "MIT", "Legge i video WebM.", "github.com/OxideAV"),
            c("cpal", "RustAudio", "Apache-2.0", "Fa suonare audio e video col tasto Play dell'anteprima.", "github.com/RustAudio/cpal"),
            c("hound", "Ruud van Asseldonk", "Apache-2.0", "Scrive i file WAV.", "github.com/ruuda/hound"),
            c("flacenc", "Ryuichi Yamamoto", "Apache-2.0", "Scrive i file FLAC.", "github.com/yotarok/flacenc-rs"),
            linux("OpenH264", "Cisco Systems", "BSD-2-Clause", "Video H.264 su Linux.", "github.com/cisco/openh264"),
            linux("FDK AAC", "Fraunhofer IIS", "Licenza FDK AAC", "Audio AAC su Linux.", "github.com/mstorsjo/fdk-aac"),
            c("whisper.cpp", "Georgi Gerganov e ggml", "MIT", "Trascrive l'audio, sul PC (programma scaricato alla prima trascrizione).", "github.com/ggml-org/whisper.cpp"),
            c("Modelli Whisper", "OpenAI", "MIT", "I modelli della trascrizione.", "github.com/openai/whisper"),
            c("whisper-rs", "Tazz4843", "Unlicense", "Collega whisper.cpp a Rust.", "codeberg.org/tazz4843/whisper-rs"),
            c("Opus", "Xiph.Org e altri", "BSD-3-Clause", "Legge e scrive i file .opus.", "opus-codec.org"),
            c("yt-dlp", "yt-dlp contributors", "Unlicense", "Scarica video e audio da YouTube e da molti altri siti (si scarica al primo uso).", "github.com/yt-dlp/yt-dlp"),
            c("FFmpeg", "Gli sviluppatori di FFmpeg", "LGPL-2.1", "Unisce video e audio dei download (si scarica al primo uso).", "ffmpeg.org"),
            c("Deno", "Deno Land Inc. e collaboratori", "MIT", "Serve a yt-dlp per i video ad alta qualità (si scarica al primo uso).", "github.com/denoland/deno"),
        ],
    },
    Group {
        title: "Interfaccia e sistema",
        items: &[
            c("windows-rs", "Microsoft", "MIT o Apache-2.0", "Le finestre e i servizi di Windows.", "github.com/microsoft/windows-rs"),
            linux("gtk4-rs", "The gtk-rs Project", "MIT", "Le finestre su Linux.", "github.com/gtk-rs/gtk4-rs"),
            linux("ksni", "iovxw", "Unlicense", "L'icona vicino all'orologio su Linux.", "github.com/iovxw/ksni"),
            c("ureq, serde_json, sha2", "Martin Algesten, serde-rs, RustCrypto", "MIT o Apache-2.0", "Controllo degli aggiornamenti.", "github.com/algesten/ureq"),
            c("tiny-skia", "Yevhenii Reizner e collaboratori", "BSD-3-Clause", "Disegna le finestre della versione standard.", "github.com/linebender/tiny-skia"),
            c("Figtree", "Erik D. Kennedy e collaboratori", "SIL OFL 1.1", "Il carattere della versione standard.", "fonts.google.com/specimen/Figtree"),
            c("Titillium Web", "Accademia di Belle Arti di Urbino", "SIL OFL 1.1", "Il carattere di Cedro.", "fonts.google.com/specimen/Titillium+Web"),
        ],
    },
];

/// La versione di whisper.cpp dentro il motore: quella di whisper-rs-sys 0.15 (il test `whisper_version_matches_the_engine` avverte se cambia).
const WHISPER_CPP: &str = "1.8.3";
/// Le versioni dei programmi fissati dentro le librerie (LAME in mp3lame-sys, libopus in opusic-sys).
const LAME: &str = "3.100";
const LIBOPUS: &str = "1.6.1";

/// Le versioni delle librerie Rust di Cedro e del suo motore, decise alla compilazione.
fn crate_version(name: &str) -> Option<&'static str> {
    env!("CEDRO_CRATES").split(';').find_map(|e| e.split_once('=').filter(|(n, _)| *n == name).map(|(_, v)| v))
}

fn crates_text(names: &[&str]) -> Option<String> {
    let found: Vec<(&str, &str)> = names.iter().filter_map(|n| crate_version(n).map(|v| (*n, v))).collect();
    match found.as_slice() {
        [] => None,
        [(_, v)] => Some((*v).to_string()),
        many => Some(many.iter().map(|(n, v)| format!("{n} {v}")).collect::<Vec<_>>().join(" · ")),
    }
}

/// La versione da mostrare in piccolo accanto a un componente. Per quelli scaricati al primo uso è quella
/// davvero sul PC; se non sono ancora arrivati, quella prevista.
pub fn version_text(item: &Component) -> Option<String> {
    use crate::{trascrizione, youtube};
    let tool = |name: &str, soon: &str| match youtube::installed_version(name) {
        Some(v) => Some(v),
        None if youtube::tool_installed(name) => None,
        None => Some(soon.to_string()),
    };
    match item.name {
        "whisper.cpp" => Some(if trascrizione::engine_installed() { WHISPER_CPP.to_string() } else { format!("{WHISPER_CPP} · da scaricare") }),
        "Modelli Whisper" => {
            let have = trascrizione::installed_models();
            Some(if have.is_empty() { format!("{} · da scaricare", trascrizione::model_names(trascrizione::DEFAULT_MODEL)) } else { have.join(", ") })
        }
        "yt-dlp" => tool("yt-dlp", "ultima versione · da scaricare"),
        "FFmpeg" => tool("ffmpeg", "ultima versione · da scaricare"),
        "Deno" => tool("deno", "ultima versione · da scaricare"),
        "LAME" => Some(LAME.to_string()),
        "Opus" => Some(LIBOPUS.to_string()),
        "whisper-rs" => crates_text(&["whisper-rs"]),
        "hayro" => crates_text(&["hayro"]),
        "lopdf" => crates_text(&["lopdf"]),
        "skrifa" => crates_text(&["skrifa"]),
        "tiny-skia" => crates_text(&["tiny-skia"]),
        "cfb" => crates_text(&["cfb"]),
        "zip" => crates_text(&["zip"]),
        "image, png" => crates_text(&["image", "png"]),
        "ocrs, rten" => crates_text(&["ocrs", "rten"]),
        "Symphonia" => crates_text(&["symphonia"]),
        "cpal" => crates_text(&["cpal"]),
        "oxideav VP8 e VP9" => crates_text(&["oxideav-vp8", "oxideav-vp9"]),
        "hound" => crates_text(&["hound"]),
        "flacenc" => crates_text(&["flacenc"]),
        "resvg" => crates_text(&["resvg"]),
        "ravif, rav1e, avif-decode, rav1d" => crates_text(&["ravif", "avif-decode", "rav1d"]),
        "jxl-oxide, zune-jpegxl" => crates_text(&["jxl-oxide", "zune-jpegxl"]),
        "windows-rs" => crates_text(&["windows"]),
        "gtk4-rs" => crates_text(&["gtk4"]),
        "ksni" => crates_text(&["ksni"]),
        "ureq, serde_json, sha2" => crates_text(&["ureq", "serde_json", "sha2"]),
        _ => None,
    }
}

/// Il testo (o l'avviso breve) della licenza, per chi apre una voce. Il testo completo sta nel file «Licenze di terze parti».
pub fn license_text(license: &str) -> &'static str {
    let key = license.split(" o ").next().unwrap_or(license).trim();
    match key {
        "MIT" => MIT,
        "Apache-2.0" => APACHE,
        "BSD-2-Clause" => BSD2,
        "BSD-3-Clause" => BSD3,
        "MPL-2.0" => MPL,
        "LGPL-3.0" => LGPL,
        "LGPL-2.1" => LGPL21,
        "SIL OFL 1.1" => OFL,
        "Unlicense" => UNLICENSE,
        "CC BY-SA 4.0" => CC_BY_SA,
        "Licenza FDK AAC" => FDK,
        _ => MIT,
    }
}

/// Le voci che si vedono su questa piattaforma, con i gruppi vuoti tolti.
pub fn visible() -> Vec<(&'static str, Vec<&'static Component>)> {
    GROUPS
        .iter()
        .map(|g| (g.title, g.items.iter().filter(|i| !i.linux_only || cfg!(target_os = "linux")).collect::<Vec<_>>()))
        .filter(|(_, items)| !items.is_empty())
        .collect()
}

/// Quanti componenti elenca il file completo delle licenze, dalla riga «Le librerie sono N» in testa.
pub fn count_in(text: &str) -> Option<usize> {
    let rest = text.lines().take(8).find_map(|l| l.strip_prefix("Le librerie sono "))?;
    rest.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()
}

/// Il file completo accanto all'eseguibile, se c'è.
pub fn full_file() -> Option<std::path::PathBuf> {
    let path = std::env::current_exe().ok()?.parent()?.join(FULL_FILE);
    path.exists().then_some(path)
}

/// Quanti componenti ha il file completo accanto all'eseguibile; `None` se il file manca.
pub fn full_count() -> Option<usize> {
    count_in(&std::fs::read_to_string(full_file()?).ok()?)
}

/// Il nome del file con tutte le licenze, accanto all'eseguibile e nel sito.
pub const FULL_FILE: &str = "Licenze di terze parti.txt";

const MIT: &str = "Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated documentation files (the \"Software\"), to deal in the Software without restriction, including without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the following conditions: the above copyright notice and this permission notice shall be included in all copies or substantial portions of the Software. THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND.";
const APACHE: &str = "Licensed under the Apache License, Version 2.0 (the \"License\"); you may not use this file except in compliance with the License. You may obtain a copy of the License at http://www.apache.org/licenses/LICENSE-2.0. Unless required by applicable law or agreed to in writing, software distributed under the License is distributed on an \"AS IS\" BASIS, WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.";
const BSD3: &str = "Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met: 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer. 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution. 3. Neither the name of the copyright holders nor the names of its contributors may be used to endorse or promote products derived from this software without specific prior written permission.";

const BSD2: &str = "Redistribution and use in source and binary forms, with or without modification, are permitted provided that the following conditions are met: 1. Redistributions of source code must retain the above copyright notice, this list of conditions and the following disclaimer. 2. Redistributions in binary form must reproduce the above copyright notice, this list of conditions and the following disclaimer in the documentation and/or other materials provided with the distribution.";
const MPL: &str = "This Source Code Form is subject to the terms of the Mozilla Public License, v. 2.0. If a copy of the MPL was not distributed with this file, You can obtain one at https://mozilla.org/MPL/2.0/.";
const LGPL: &str = "This library is free software; you can redistribute it and/or modify it under the terms of the GNU Lesser General Public License as published by the Free Software Foundation; either version 3 of the License, or (at your option) any later version. This library is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY.";
const LGPL21: &str = "This library is free software; you can redistribute it and/or modify it under the terms of the GNU Lesser General Public License as published by the Free Software Foundation; either version 2.1 of the License, or (at your option) any later version. This library is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the GNU Lesser General Public License for more details. Cedro uses the \"lgpl\" builds of BtbN (github.com/BtbN/FFmpeg-Builds) and runs FFmpeg as a separate program; the source code is available at ffmpeg.org.";
const OFL: &str = "This Font Software is licensed under the SIL Open Font License, Version 1.1. This license is available with a FAQ at https://openfontlicense.org. The fonts may be used, studied, modified and redistributed freely as long as they are not sold by themselves.";
const UNLICENSE: &str = "This is free and unencumbered software released into the public domain. Anyone is free to copy, modify, publish, use, compile, sell, or distribute this software, either in source code form or as a compiled binary, for any purpose, commercial or non-commercial, and by any means.";
const CC_BY_SA: &str = "This work is licensed under the Creative Commons Attribution-ShareAlike 4.0 International License. To view a copy of this license, visit https://creativecommons.org/licenses/by-sa/4.0/. You may share and adapt the material for any purpose, even commercially, giving appropriate credit and distributing your contributions under the same license.";
const FDK: &str = "Software License for The Fraunhofer FDK AAC Codec Library for Android. Redistribution and use in source and binary forms, with or without modification, are permitted provided that the conditions of the license are met. No patent license is granted by this license; use of AAC may require a separate patent license.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_count_comes_from_the_header() {
        let head = "LICENZE DI TERZE PARTI - Cedro 0.17.0\r\nCedro è software libero.\r\nLe librerie sono 251; i testi uguali compaiono una volta sola.\r\n";
        assert_eq!(count_in(head), Some(251));
        assert_eq!(count_in("niente"), None);
    }

    #[test]
    fn whisper_version_matches_the_engine() {
        // whisper.cpp arriva dentro whisper-rs-sys: se cambia la versione, controllare WHISPER_CPP, LAME e LIBOPUS.
        let lock = include_str!("../trascrivi/Cargo.lock");
        let at = lock.find("name = \"whisper-rs-sys\"").expect("whisper-rs-sys");
        assert!(lock[at..].lines().nth(1).unwrap().contains("\"0.15."), "whisper-rs-sys è cambiata: aggiorna WHISPER_CPP");
        assert_eq!(WHISPER_CPP, "1.8.3");
    }

    #[test]
    #[cfg(any(windows, target_os = "linux", target_os = "android"))]
    fn libopus_version_is_the_one_shown() {
        // libopus è compilata dentro opusic-sys: i riconoscimenti mostrano la sua versione.
        let v = unsafe { std::ffi::CStr::from_ptr(opusic_sys::opus_get_version_string()) };
        assert_eq!(v.to_str().unwrap(), format!("libopus {LIBOPUS}"));
    }

    #[test]
    fn engine_version_follows_cedro() {
        // Cedro scarica il motore dalla release con il proprio numero di versione: build.ps1 lo prende da qui.
        let toml = include_str!("../trascrivi/Cargo.toml");
        let line = toml.lines().find(|l| l.starts_with("version = ")).expect("version");
        assert_eq!(line, format!("version = \"{}\"", env!("CARGO_PKG_VERSION")));
    }

    #[test]
    fn library_versions_are_found() {
        assert!(crate_version("hound").is_some());
        assert!(crate_version("whisper-rs").is_some());
        assert_eq!(crate_version("windows").map(|v| v.starts_with("0.61")), Some(true));
        let all: Vec<_> = GROUPS.iter().flat_map(|g| g.items.iter()).filter(|i| !i.linux_only).collect();
        for item in all {
            if ["Modelli OCR di ocrs", "Titillium Web", "Figtree"].contains(&item.name) {
                continue;
            }
            assert!(version_text(item).is_some() || ["yt-dlp", "FFmpeg", "Deno"].contains(&item.name), "{} senza versione", item.name);
        }
    }

    #[test]
    fn every_license_has_a_text() {
        for g in GROUPS {
            for item in g.items {
                let text = license_text(item.license);
                assert!(text.len() > 60, "{} senza testo", item.name);
            }
        }
    }

    #[test]
    fn windows_hides_linux_only_items() {
        let all: usize = GROUPS.iter().map(|g| g.items.len()).sum();
        let shown: usize = visible().iter().map(|(_, v)| v.len()).sum();
        if cfg!(target_os = "linux") {
            assert_eq!(shown, all);
        } else {
            assert!(shown < all);
        }
    }
}
