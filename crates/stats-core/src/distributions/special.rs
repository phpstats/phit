//! Numerical pieces cdflib does not provide: log-scale tails beyond the double
//! range and logistic-scale helpers. Loader's saddle-point densities and the incomplete gamma and beta
//! continued fractions are in saddle.rs.

// Written from the clean-room specification specs/cleanroom/special.md; see saddle.rs.
pub(super) use super::saddle::{
    ln_beta_lower_fraction, ln_binomial_raw, ln_binomial_raw_split, ln_gamma_lower_series,
    ln_gamma_upper_fraction, ln_poisson_raw,
};

/// Probabilities below this are recomputed on the log scale instead of taking
/// ln(p), so log tails stay accurate after p itself underflows.
pub(super) const TINY: f64 = 1e-280;
pub(super) const LN_SQRT_2PI: f64 = 0.918_938_533_204_672_8;

/// ln(lower) from independently computed complementary probabilities.
/// `extreme` supplies ln(lower) once `lower` is too small for ln(lower);
/// it returns NaN outside its verified regime, which callers report as an error.
pub(super) fn log_tail(lower: f64, upper: f64, extreme: impl FnOnce() -> f64) -> f64 {
    if lower > 0.5 {
        (-upper).ln_1p()
    } else if lower >= TINY {
        lower.ln()
    } else {
        extreme()
    }
}

/// ln Φ(z) for z < -8 from the continued fraction for the Mills ratio.
/// The probit estimator retains the same continued-fraction convention in its
/// established numerical path; see the recorded distribution reuse audit.
pub(crate) fn normal_log_lower_tail(z: f64) -> f64 {
    let x = -z;
    if x.is_infinite() {
        return f64::NEG_INFINITY;
    }
    let mut denominator = x;
    for k in (1..=100).rev() {
        denominator = x + k as f64 / denominator;
    }
    -0.5 * x * x - LN_SQRT_2PI - denominator.ln()
}

/// ln(1 + e^t) without overflow.
pub(super) fn softplus(t: f64) -> f64 {
    t.max(0.0) + (-t.abs()).exp().ln_1p()
}

/// ln(1 - e^-t) for t ≥ 0 (Mächler's log1mexp).
pub(super) fn ln_one_minus_exp_neg(t: f64) -> f64 {
    if t <= std::f64::consts::LN_2 {
        (-(-t).exp_m1()).ln()
    } else {
        (-(-t).exp()).ln_1p()
    }
}

#[cfg(test)]
mod tests {
    use super::super::saddle::{deviance_term, stirling_error};
    use super::*;

    #[test]
    fn stirling_error_matches_log_gamma_definition() {
        for n in [0.25, 3.7, 12.0, 15.5, 20.0, 40.0, 100.0, 1000.0] {
            let direct = statrs::function::gamma::ln_gamma(n + 1.0) - (n + 0.5) * f64::ln(n) + n
                - LN_SQRT_2PI;
            assert!((stirling_error(n) - direct).abs() < 1e-12, "{n}");
        }
    }

    #[test]
    fn deviance_term_is_nonnegative_and_stable() {
        assert_eq!(deviance_term(5.0, 5.0), 0.0);
        assert!(deviance_term(1e6, 1e6 + 1.0) > 0.0);
        let direct = 7.0 * (7.0f64 / 3.0).ln() + 3.0 - 7.0;
        assert!((deviance_term(7.0, 3.0) - direct).abs() < 1e-14);
    }
}
