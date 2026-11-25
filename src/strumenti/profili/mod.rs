//! I profili degli strumenti: ognuno riconosce il file dal contenuto e lo legge. Un profilo c'è
//! solo se legge giusti tutti i file d'esempio pubblici di allotropy (vedi `tests/strumenti.rs`).

use super::Profilo;

mod cfx;
mod chemometec;
mod conta;
mod genesys;
mod gen5;
mod nanodrop_8;
mod nanodrop_one;
mod quantstudio;
mod tabella;
mod qubit;
mod qubit_flex;
mod revvity_matrix;
mod softmax_pro;
mod roche_cedex;
mod tecan;
mod vi_cell;
mod visionlite;

pub const TUTTI: &[Profilo] = &[tecan::PROFILO, cfx::PROFILO, qubit::PROFILO, nanodrop_one::PROFILO, quantstudio::PROFILO, nanodrop_8::OTTO_MILA, nanodrop_8::EIGHT, qubit_flex::PROFILO, genesys::PROFILO, visionlite::VISIONLITE, visionlite::GENESYS_ON_BOARD, vi_cell::BLU, vi_cell::XR, chemometec::NC_VIEW, chemometec::NUCLEOVIEW, revvity_matrix::PROFILO, roche_cedex::PROFILO, gen5::PROFILO, softmax_pro::PROFILO];
