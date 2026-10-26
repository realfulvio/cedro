mod audio;
pub mod conversioni;
mod doctext;
mod engine;
mod mp4fast;
mod ocr;
mod pdf;
mod pdfdoc;
mod pdflayout;
pub mod settings;
mod sink;
mod textconv;
pub mod trascrizione;
mod softvideo;
pub mod update;
pub mod wheelgeom;
pub mod wheelgfx;
mod zipstore;
#[cfg(windows)]
mod video;
#[cfg(target_os = "linux")]
mod linux_video;
#[cfg(target_os = "linux")]
mod mkv;
#[cfg(target_os = "linux")]
mod mp4mux;

pub use engine::{
    append_log, config_dir, perform_with, Exists, ExistsAsk, ExistsChoice, Options, PageSel, unique_output, extension, kind_of, mode_from_vk, perform, perform_into, read_quality, same_kind_paths, slices, Kind,
    Mode, Output, Slice,
};
#[cfg(windows)]
pub use ocr::windows_italian_installed;
pub use settings::{read_settings, write_settings, Dest, Settings};
pub use wheelgeom::{hover_at as wheelgeom_hover, slice_bounds as wheelgeom_bounds};
