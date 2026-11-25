//! I profili degli strumenti: ognuno riconosce il file dal contenuto e lo legge. Un profilo c'è
//! solo se legge giusti tutti i file d'esempio pubblici di allotropy (vedi `tests/strumenti.rs`).

use super::Profilo;

mod cfx;
mod nanodrop_one;
mod qubit;
mod tecan;

pub const TUTTI: &[Profilo] = &[tecan::PROFILO, cfx::PROFILO, qubit::PROFILO, nanodrop_one::PROFILO];
