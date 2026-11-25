//! I profili degli strumenti: ognuno riconosce il file dal contenuto e lo legge. Un profilo c'è
//! solo se legge giusti tutti i file d'esempio pubblici di allotropy (vedi `tests/strumenti.rs`).

use super::Profilo;

mod cfx;
mod genesys;
mod nanodrop_8;
mod nanodrop_one;
mod quantstudio;
mod tabella;
mod qubit;
mod qubit_flex;
mod tecan;
mod visionlite;

pub const TUTTI: &[Profilo] = &[tecan::PROFILO, cfx::PROFILO, qubit::PROFILO, nanodrop_one::PROFILO, quantstudio::PROFILO, nanodrop_8::OTTO_MILA, nanodrop_8::EIGHT, qubit_flex::PROFILO, genesys::PROFILO, visionlite::VISIONLITE, visionlite::GENESYS_ON_BOARD];
