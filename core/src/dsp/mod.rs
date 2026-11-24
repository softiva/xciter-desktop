//! Xciter's sound: the same engine as the Xciter apps on Android and iPhone, so every device sounds the same: same
//! constants, same filter order, same arithmetic. Change it only together with the other implementations.
use std::f64::consts::PI;

mod air;
mod effects;
mod eq;
mod filters;
mod meter;
mod output;
mod settings;

pub use air::*;
pub use effects::*;
pub use eq::*;
pub use filters::*;
pub use meter::*;
pub use output::*;
pub use settings::*;

#[cfg(test)]
mod tests;
