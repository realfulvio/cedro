//! Documenti d'ufficio stampati come li stampa il programma che li ha scritti: Word, ODT, RTF e DOC
//! con tabelle, stili, elenchi, intestazioni e note; Excel, Calc e XLS con le celle formattate, le
//! pagine e le impostazioni di stampa del file. Tutto in Rust, con i caratteri del sistema.

pub mod font;
pub mod pagina;
pub mod pdf;
pub mod numfmt;
pub mod foglio;
pub mod stampa_fogli;
