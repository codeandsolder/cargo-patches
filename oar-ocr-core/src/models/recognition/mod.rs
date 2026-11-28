//! Recognition model implementations.
//!
//! This module contains pure model implementations for recognition tasks.

pub mod crnn;
#[cfg(feature = "cuda")]
mod ctc_cuda;
#[cfg(feature = "formula-recognition")]
pub mod pp_formulanet;
pub mod slanet;
#[cfg(feature = "formula-recognition")]
pub mod unimernet;

pub use crnn::{CRNNModel, CRNNModelBuilder, CRNNModelOutput, CRNNPreprocessConfig};
#[cfg(feature = "formula-recognition")]
pub use pp_formulanet::{
    PPFormulaNetModel, PPFormulaNetModelBuilder, PPFormulaNetModelOutput,
    PPFormulaNetPostprocessConfig, PPFormulaNetPreprocessConfig,
};
pub use slanet::{SLANetModel, SLANetModelBuilder, SLANetModelOutput};
#[cfg(feature = "formula-recognition")]
pub use unimernet::{
    UniMERNetModel, UniMERNetModelBuilder, UniMERNetModelOutput, UniMERNetPostprocessConfig,
    UniMERNetPreprocessConfig,
};
