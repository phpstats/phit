//! Pure Rust statistical calculations, independent of PHP.
pub mod csv;
pub mod descriptive;
pub mod distributions;
pub mod draws;
pub mod formula;
pub mod logistic;
pub mod ols;
pub mod smoothing;
pub use logistic::{LogisticResult, fit_logistic};
pub use ols::{OlsResult, fit_ols};
mod error;
pub use error::{ErrorKind, StatsError};

mod poisson;
mod probit;
pub use poisson::{PoissonResult, fit_poisson};
pub use probit::{ProbitResult, fit_probit};

mod mixed;
pub use mixed::{MixedResult, fit_random_intercept};

mod lmm;
pub use lmm::{LmmOptimizer, LmmOptions, LmmResult, RandomTerm, RandomTermResult, fit_lmm};

mod observations;
pub use logistic::fit_logistic_with_data;
pub use ols::fit_ols_with_data;
pub use poisson::fit_poisson_with_data;
pub use probit::fit_probit_with_data;

mod logistic_mixed;
pub use logistic_mixed::{
    LogisticMixedOptions, LogisticMixedResult, MAX_LOGISTIC_MIXED_TRIALS,
    fit_logistic_random_intercept,
};
