//! Probability distributions: densities, log densities, CDFs, survival
//! functions S(x) = P(X > x), their logarithms and quantiles.
//!
//! Tail probabilities come from cdflib's ports of TOMS 708 (incomplete beta),
//! TOMS 654 (incomplete gamma) and Cody's normal CDF, which compute the lower
//! and upper tails independently. This module adds log-scale tails beyond the
//! double range, Loader's saddle-point densities, accurate quantiles and the
//! input/result policy shared by the scalar and batch PHP functions:
//!
//! - parameters must be finite and in their documented domain;
//! - evaluation points may be ±∞ but not NaN;
//! - quantile probabilities must lie in [0, 1];
//! - out-of-support points give density 0 (log density -∞), never an error;
//! - a NaN result is a numerical failure and is reported as an error.
mod continuous;
mod discrete;
mod quantile;
mod rng;
mod special;

pub use continuous::{
    Beta, ChiSquare, Exponential, FisherF, Gamma, LogNormal, Logistic, Normal, StudentT,
};
pub use discrete::{Binomial, MAX_BINOMIAL_SIZE, NegativeBinomial, Poisson};
pub use rng::{
    MAX_RNG_DRAWS, sample_categorical, sample_categorical_rows, sample_permutation,
    sample_quantiles, sample_uniform,
};

use crate::{ErrorKind, StatsError};

// cdflib's TOMS 708/654 ports never return for some extreme arguments (their
// continued fractions spin once a NaN or overflow appears) and panic in the deep
// asymptotic regime of the incomplete gamma. Fuzzing found the hanging regimes:
// an overflowing beta a + b, a beta parameter above 1e300 with x or 1 - x below
// 1e-300, and a gamma shape or point above 1e300. Those return NaN up front, and
// any remaining panic becomes NaN; the distributions report NaN as a numerical
// failure (code 1006).
const CDFLIB_ARGUMENT_LIMIT: f64 = 1e300;

fn without_panic(evaluate: impl FnOnce() -> (f64, f64)) -> (f64, f64) {
    std::panic::catch_unwind(std::panic::AssertUnwindSafe(evaluate)).unwrap_or((f64::NAN, f64::NAN))
}

pub(crate) fn beta_inc(a: f64, b: f64, x: f64, y: f64) -> (f64, f64) {
    if !(a + b).is_finite()
        || (a.max(b) > CDFLIB_ARGUMENT_LIMIT && x.min(y) < 1.0 / CDFLIB_ARGUMENT_LIMIT)
    {
        return (f64::NAN, f64::NAN);
    }
    without_panic(|| cdflib::special::beta_inc(a, b, x, y))
}

pub(crate) fn gamma_inc(a: f64, x: f64) -> (f64, f64) {
    if a.is_nan() || x.is_nan() || a > CDFLIB_ARGUMENT_LIMIT || x > CDFLIB_ARGUMENT_LIMIT {
        return (f64::NAN, f64::NAN);
    }
    without_panic(|| cdflib::special::gamma_inc(a, x))
}

pub(crate) fn finite(name: &str, value: f64) -> Result<f64, StatsError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(StatsError::new(
            ErrorKind::Nonfinite,
            format!("{name} must be finite"),
        ))
    }
}

pub(crate) fn positive(name: &str, value: f64) -> Result<f64, StatsError> {
    if finite(name, value)? > 0.0 {
        Ok(value)
    } else {
        Err(StatsError::new(
            ErrorKind::InvalidInput,
            format!("{name} must be positive"),
        ))
    }
}

fn result(value: f64) -> Result<f64, StatsError> {
    if value.is_nan() {
        Err(StatsError::new(
            ErrorKind::Nonconvergence,
            "numerical evaluation failed for these arguments",
        ))
    } else {
        Ok(value)
    }
}

/// Evaluate a density, CDF or survival function at x (±∞ allowed, NaN rejected).
pub fn evaluate(x: f64, f: impl Fn(f64) -> f64) -> Result<f64, StatsError> {
    if x.is_nan() {
        return Err(StatsError::new(
            ErrorKind::Nonfinite,
            "evaluation points must not be NaN",
        ));
    }
    result(f(x))
}

/// Evaluate a quantile function at a probability in [0, 1].
pub fn evaluate_quantile(p: f64, f: impl Fn(f64) -> f64) -> Result<f64, StatsError> {
    if p.is_nan() {
        return Err(StatsError::new(
            ErrorKind::Nonfinite,
            "probabilities must not be NaN",
        ));
    }
    if !(0.0..=1.0).contains(&p) {
        return Err(StatsError::new(
            ErrorKind::InvalidInput,
            "probabilities must be in [0, 1]",
        ));
    }
    result(f(p))
}

/// Batch form of [`evaluate`]: one output per input, in order.
pub fn evaluate_all(values: &[f64], f: impl Fn(f64) -> f64) -> Result<Vec<f64>, StatsError> {
    values.iter().map(|&x| evaluate(x, &f)).collect()
}

/// Batch form of [`evaluate_quantile`]: one output per input, in order.
pub fn evaluate_quantiles(
    probabilities: &[f64],
    f: impl Fn(f64) -> f64,
) -> Result<Vec<f64>, StatsError> {
    probabilities
        .iter()
        .map(|&p| evaluate_quantile(p, &f))
        .collect()
}
