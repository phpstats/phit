//! Descriptive statistics of numeric slices.
//!
//! Inputs must be finite; dropping or rejecting missing values is the caller's
//! policy. Sums are compensated (Neumaier) and second moments use the corrected
//! two-pass algorithm, so large offsets do not cancel the variability.
use crate::{ErrorKind, StatsError};

/// Weighted variance definitions; W = Σw and m is the weighted mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeightKind {
    /// Weights are replication counts: Σw(x - m)² / (W - 1).
    Frequency,
    /// Weights are precisions/reliabilities: Σw(x - m)² / (W - Σw²/W).
    Reliability,
    /// Weighted population variance: Σw(x - m)² / W.
    Population,
}

fn error(kind: ErrorKind, message: &str) -> StatsError {
    StatsError::new(kind, message)
}

fn computed(value: f64) -> Result<f64, StatsError> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(error(
            ErrorKind::Nonfinite,
            "descriptive calculation exceeded floating-point range",
        ))
    }
}

fn check(values: &[f64], minimum: usize, what: &str) -> Result<(), StatsError> {
    if values.iter().any(|v| !v.is_finite()) {
        return Err(error(ErrorKind::Nonfinite, "values must be finite"));
    }
    if values.len() < minimum {
        let count = if minimum == 1 {
            "one value"
        } else {
            "two values"
        };
        return Err(StatsError::new(
            ErrorKind::InvalidInput,
            format!("{what} requires at least {count}"),
        ));
    }
    Ok(())
}

fn check_pair(x: &[f64], y: &[f64], what: &str) -> Result<(), StatsError> {
    if x.len() != y.len() {
        return Err(error(
            ErrorKind::DimensionMismatch,
            "x and y must have the same length",
        ));
    }
    check(x, 2, what)?;
    check(y, 2, what)
}

/// Neumaier compensated sum.
fn sum(values: impl Iterator<Item = f64>) -> f64 {
    let (mut total, mut compensation) = (0.0, 0.0);
    for v in values {
        let t = total + v;
        if total.abs() >= v.abs() {
            compensation += (total - t) + v;
        } else {
            compensation += (v - t) + total;
        }
        total = t;
    }
    total + compensation
}

fn plain_mean(values: &[f64]) -> f64 {
    let n = values.len() as f64;
    let m = sum(values.iter().copied()) / n;
    // Second pass corrects rounding in the first, as R's mean() does.
    m + sum(values.iter().map(|v| v - m)) / n
}

/// Arithmetic mean.
pub fn mean(values: &[f64]) -> Result<f64, StatsError> {
    check(values, 1, "mean")?;
    computed(plain_mean(values))
}

// Corrected two-pass sum of squared deviations from the mean.
fn squared_deviations(values: &[f64]) -> Result<f64, StatsError> {
    let m = plain_mean(values);
    let ss = sum(values.iter().map(|v| (v - m) * (v - m)));
    let s = sum(values.iter().map(|v| v - m));
    // Check before max: f64::max(NAN, 0) would hide overflow as zero variance.
    Ok(computed(ss - s * s / values.len() as f64)?.max(0.0))
}

/// Variance with denominator n - 1 (sample) or n (population).
pub fn variance(values: &[f64], population: bool) -> Result<f64, StatsError> {
    check(values, if population { 1 } else { 2 }, "variance")?;
    let n = values.len() as f64;
    computed(squared_deviations(values)? / if population { n } else { n - 1.0 })
}

/// Standard deviation: the square root of [`variance`].
pub fn sd(values: &[f64], population: bool) -> Result<f64, StatsError> {
    Ok(variance(values, population)?.sqrt())
}

// Sums of centered cross-products (Sxy, Sxx, Syy).
fn cross_products(x: &[f64], y: &[f64]) -> Result<(f64, f64, f64), StatsError> {
    let (mx, my) = (plain_mean(x), plain_mean(y));
    let n = x.len() as f64;
    let dx = || x.iter().map(move |v| v - mx);
    let dy = || y.iter().map(move |v| v - my);
    let (sx, sy) = (sum(dx()), sum(dy()));
    let sxy = computed(sum(dx().zip(dy()).map(|(a, b)| a * b)) - sx * sy / n)?;
    let sxx = computed(sum(dx().map(|a| a * a)) - sx * sx / n)?.max(0.0);
    let syy = computed(sum(dy().map(|b| b * b)) - sy * sy / n)?.max(0.0);
    Ok((sxy, sxx, syy))
}

/// Covariance with denominator n - 1 (sample) or n (population).
pub fn covariance(x: &[f64], y: &[f64], population: bool) -> Result<f64, StatsError> {
    check_pair(x, y, "covariance")?;
    let n = x.len() as f64;
    computed(cross_products(x, y)?.0 / if population { n } else { n - 1.0 })
}

/// Pearson correlation; NaN when either input has zero variance (undefined).
pub fn correlation(x: &[f64], y: &[f64]) -> Result<f64, StatsError> {
    check_pair(x, y, "correlation")?;
    let (sxy, sxx, syy) = cross_products(x, y)?;
    if sxx == 0.0 || syy == 0.0 {
        return Ok(f64::NAN);
    }
    // Sequential division avoids overflow of the product of standard deviations.
    Ok(computed((sxy / sxx.sqrt()) / syy.sqrt())?.clamp(-1.0, 1.0))
}

fn check_probability(p: f64) -> Result<(), StatsError> {
    if p.is_nan() {
        return Err(error(ErrorKind::Nonfinite, "probabilities must not be NaN"));
    }
    if !(0.0..=1.0).contains(&p) {
        return Err(error(
            ErrorKind::InvalidInput,
            "probabilities must be in [0, 1]",
        ));
    }
    Ok(())
}

/// Sample quantile by R's default definition (type 7, linear interpolation).
pub fn quantile(values: &[f64], p: f64) -> Result<f64, StatsError> {
    Ok(quantiles(values, &[p])?[0])
}

/// Several type 7 quantiles from one sort.
pub fn quantiles(values: &[f64], probabilities: &[f64]) -> Result<Vec<f64>, StatsError> {
    check(values, 1, "quantile")?;
    for &p in probabilities {
        check_probability(p)?;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable_by(f64::total_cmp);
    Ok(probabilities
        .iter()
        .map(|&p| crate::numeric::quantile_type7(&sorted, p))
        .collect())
}

/// Median: the type 7 quantile at 0.5 (the usual midpoint rule).
pub fn median(values: &[f64]) -> Result<f64, StatsError> {
    quantile(values, 0.5)
}

fn check_weights(values: &[f64], weights: &[f64]) -> Result<f64, StatsError> {
    if values.len() != weights.len() {
        return Err(error(
            ErrorKind::DimensionMismatch,
            "values and weights must have the same length",
        ));
    }
    check(values, 1, "weighted statistics")?;
    if weights.iter().any(|w| !w.is_finite()) {
        return Err(error(ErrorKind::Nonfinite, "weights must be finite"));
    }
    if weights.iter().any(|&w| w < 0.0) {
        return Err(error(
            ErrorKind::InvalidInput,
            "weights must be nonnegative",
        ));
    }
    let total = computed(sum(weights.iter().copied()))?;
    if total <= 0.0 {
        return Err(error(
            ErrorKind::InvalidInput,
            "weights must have a positive sum",
        ));
    }
    Ok(total)
}

fn plain_weighted_mean(values: &[f64], weights: &[f64], total: f64) -> f64 {
    let m = sum(values.iter().zip(weights).map(|(v, w)| v * w)) / total;
    m + sum(values.iter().zip(weights).map(|(v, w)| w * (v - m))) / total
}

/// Weighted mean Σw x / Σw.
pub fn weighted_mean(values: &[f64], weights: &[f64]) -> Result<f64, StatsError> {
    let total = check_weights(values, weights)?;
    computed(plain_weighted_mean(values, weights, total))
}

/// Weighted variance under the chosen [`WeightKind`] definition.
pub fn weighted_variance(
    values: &[f64],
    weights: &[f64],
    kind: WeightKind,
) -> Result<f64, StatsError> {
    let total = check_weights(values, weights)?;
    let m = plain_weighted_mean(values, weights, total);
    let ss = sum(values
        .iter()
        .zip(weights)
        .map(|(v, w)| w * (v - m) * (v - m)));
    let s = sum(values.iter().zip(weights).map(|(v, w)| w * (v - m)));
    let ss = computed(ss - s * s / total)?.max(0.0);
    let denominator = computed(match kind {
        WeightKind::Frequency => total - 1.0,
        WeightKind::Reliability => total - sum(weights.iter().map(|w| w * w)) / total,
        WeightKind::Population => total,
    })?;
    if denominator <= 0.0 {
        return Err(error(
            ErrorKind::InvalidInput,
            match kind {
                WeightKind::Frequency => "frequency weights must sum to more than 1",
                _ => "reliability weights need at least two positive weights",
            },
        ));
    }
    computed(ss / denominator)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn large_offsets_do_not_cancel() {
        let values = [1e9 + 4.0, 1e9 + 7.0, 1e9 + 13.0, 1e9 + 16.0];
        assert_eq!(variance(&values, false).unwrap(), 30.0);
        assert_eq!(mean(&values).unwrap(), 1e9 + 10.0);
    }

    #[test]
    fn insufficient_and_degenerate_inputs() {
        assert!(mean(&[]).is_err());
        assert!(variance(&[1.0], false).is_err());
        assert_eq!(variance(&[1.0], true).unwrap(), 0.0);
        assert!(
            correlation(&[1.0, 1.0, 1.0], &[1.0, 2.0, 3.0])
                .unwrap()
                .is_nan()
        );
        assert!(quantile(&[1.0, 2.0], 1.5).is_err());
        assert!(weighted_variance(&[1.0, 2.0], &[0.0, 0.5], WeightKind::Frequency).is_err());
    }
}
