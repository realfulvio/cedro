pub mod accel;
mod audio;
pub mod progress;
pub mod conversioni;
pub mod credits;
pub mod edizione;
pub mod guida;
pub mod logo;
mod doctext;
mod engine;
mod mp4fast;
mod ocr;
mod pdf;
mod pdfdoc;
mod pdflayout;
pub mod settings;
pub mod tema;
pub mod ui;
pub mod sysinfo;
mod sink;
mod textconv;
pub mod trascrizione;
mod softvideo;
pub mod update;
pub mod vista;
pub mod wheelgeom;
pub mod wheelgfx;
pub mod webvideo;
pub mod youtube;
mod zipstore;
#[cfg(windows)]
mod video;
#[cfg(any(target_os = "linux", target_os = "android"))]
mod linux_video;
#[cfg(any(target_os = "linux", target_os = "android"))]
mod mkv;
#[cfg(any(target_os = "linux", target_os = "android"))]
mod mp4mux;
#[cfg(target_os = "linux")]
mod vaapi;
#[cfg(target_os = "android")]
mod mediacodec;
#[cfg(target_os = "android")]
mod whisper_android;
#[cfg(target_os = "android")]
mod mediadec;
/// Il codificatore H.264 della scheda video: VA-API su Linux, MediaCodec su Android.
#[cfg(target_os = "linux")]
pub(crate) use vaapi as gpuenc;
#[cfg(target_os = "android")]
pub(crate) use mediacodec as gpuenc;

pub use engine::{
    append_log, config_dir, perform_with, Exists, ExistsAsk, ExistsChoice, Options, PageSel, unique_output, extension, kind_of, mode_from_vk, perform, perform_into, read_quality, same_kind_paths, slices, Kind,
    Mode, Output, Slice,
};
pub use audio::waveform;
#[cfg(windows)]
pub use ocr::windows_italian_installed;
pub use settings::{read_settings, write_settings, Dest, Settings};
pub use wheelgeom::{hover_at as wheelgeom_hover, slice_bounds as wheelgeom_bounds};
