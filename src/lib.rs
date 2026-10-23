mod audio;
mod doctext;
mod engine;
mod ocr;
mod pdf;
pub mod settings;
mod sink;
mod softvideo;
pub mod update;
pub mod wheelgeom;
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
    append_log, config_dir, extension, kind_of, mode_from_vk, perform, read_quality, same_kind_paths, slices, Kind,
    Mode, Output, Slice,
};
pub use settings::{read_settings, write_settings, Settings};
pub use wheelgeom::{hover_at as wheelgeom_hover, slice_bounds as wheelgeom_bounds};
