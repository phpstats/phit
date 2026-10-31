//! Explicit local ChaCha20 state and inverse-CDF batch sampling.
//! No global state, entropy access or changes to the estimator RNG dependencies.
use crate::{ErrorKind, StatsError};
use rand::{Rng, SeedableRng, distr::Open01};
use rand_chacha::ChaCha20Rng;

/// Bound a single returned numeric array's allocation/work.
pub const MAX_RNG_DRAWS: usize = 1_000_000;

fn count(n: usize, message: &str) -> Result<usize, StatsError> {
    if n > MAX_RNG_DRAWS {
        return Err(StatsError::new(ErrorKind::Unsupported, message));
    }
    Ok(n)
}

const TOO_MANY: &str = "n must not exceed 1,000,000 samples per call";

/// The open-interval uniforms every sampler consumes, one per output, from a
/// ChaCha20 stream restarted at the seed. Open01 excludes 0 and 1, so
/// unbounded-support quantile endpoints cannot create artificial infinities.
/// It uses 52 random bits for f64.
fn uniforms(seed: u64) -> impl Iterator<Item = f64> {
    let mut rng = ChaCha20Rng::seed_from_u64(seed);
    std::iter::repeat_with(move || rng.sample(Open01))
}

/// Sample a validated distribution by mapping open-interval uniforms through
/// its quantile. Every call restarts the explicitly supplied seed; the same
/// seed/parameters reproduce prefixes within the pinned algorithm/version.
/// Nonfinite generated values reject the entire call (no partial output).
pub fn sample_quantiles(
    n: usize,
    seed: u64,
    quantile: impl Fn(f64) -> f64,
) -> Result<Vec<f64>, StatsError> {
    let n = count(n, TOO_MANY)?;
    let mut out = Vec::with_capacity(n);
    for p in uniforms(seed).take(n) {
        let value = quantile(p);
        if !value.is_finite() {
            return Err(StatsError::new(
                ErrorKind::Nonfinite,
                "sample exceeded floating-point range for these parameters",
            ));
        }
        out.push(value);
    }
    Ok(out)
}

/// Uniform(min, max) samples as R's runif(): min + (max - min) * u.
/// min == max gives min, as in R.
pub fn sample_uniform(n: usize, seed: u64, min: f64, max: f64) -> Result<Vec<f64>, StatsError> {
    super::finite("min", min)?;
    super::finite("max", max)?;
    if min > max {
        return Err(StatsError::new(
            ErrorKind::InvalidInput,
            "min must not exceed max",
        ));
    }
    sample_quantiles(n, seed, |u| min + (max - min) * u)
}

/// Cumulative weights of one probability vector, validated as R's sample()
/// validates prob: finite, nonnegative and with a positive sum. Weights need
/// not sum to one; each is divided by their sum.
fn cumulative(prob: &[f64], name: &str) -> Result<Vec<f64>, StatsError> {
    if prob.is_empty() {
        return Err(StatsError::new(
            ErrorKind::InvalidInput,
            format!("{name} must not be empty"),
        ));
    }
    let mut total = 0.0;
    let mut out = Vec::with_capacity(prob.len());
    for &w in prob {
        super::finite(name, w).map_err(|_| {
            StatsError::new(
                ErrorKind::Nonfinite,
                format!("{name} probabilities must be finite"),
            )
        })?;
        if w < 0.0 {
            return Err(StatsError::new(
                ErrorKind::InvalidInput,
                format!("{name} probabilities must be nonnegative"),
            ));
        }
        total += w;
        out.push(total);
    }
    if !total.is_finite() {
        return Err(StatsError::new(
            ErrorKind::Nonfinite,
            format!("{name} probabilities must have a finite sum"),
        ));
    }
    if total <= 0.0 {
        return Err(StatsError::new(
            ErrorKind::InvalidInput,
            format!("{name} probabilities must have a positive sum"),
        ));
    }
    Ok(out)
}

/// The category of one uniform: the first k with u * total < cumulative[k].
/// A zero weight is never chosen; if rounding leaves u * total at or above
/// the total, the last category with positive weight is.
fn category(cumulative: &[f64], u: f64) -> usize {
    let total = cumulative[cumulative.len() - 1];
    let target = u * total;
    cumulative
        .iter()
        .position(|&c| target < c)
        .unwrap_or_else(|| {
            let mut k = cumulative.len() - 1;
            while k > 0 && cumulative[k - 1] == total {
                k -= 1;
            }
            k
        })
}

/// n 0-based category indices drawn with probabilities prob, as R's
/// sample(length(prob), n, replace = TRUE, prob = prob) less one.
pub fn sample_categorical(n: usize, seed: u64, prob: &[f64]) -> Result<Vec<usize>, StatsError> {
    let n = count(n, TOO_MANY)?;
    let cumulative = cumulative(prob, "prob")?;
    Ok(uniforms(seed)
        .take(n)
        .map(|u| category(&cumulative, u))
        .collect())
}

/// One 0-based category index per row of prob, each drawn with that row's
/// probabilities. Draw i uses the i-th uniform, so rows that all hold the
/// same vector give sample_categorical()'s result.
pub fn sample_categorical_rows(seed: u64, prob: &[Vec<f64>]) -> Result<Vec<usize>, StatsError> {
    count(prob.len(), "prob must not have more than 1,000,000 rows")?;
    let rows = prob
        .iter()
        .enumerate()
        .map(|(i, row)| cumulative(row, &format!("prob row {i}")))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(rows
        .iter()
        .zip(uniforms(seed))
        .map(|(row, u)| category(row, u))
        .collect())
}

/// size distinct 0-based indices from 0..n in random order, as R's
/// sample(n, size) less one; size = n is a permutation. A partial
/// Fisher-Yates shuffle: step i swaps position i with i + floor(u * (n - i)).
pub fn sample_permutation(n: usize, seed: u64, size: usize) -> Result<Vec<usize>, StatsError> {
    let n = count(n, TOO_MANY)?;
    if size > n {
        return Err(StatsError::new(
            ErrorKind::InvalidInput,
            "size must not exceed n",
        ));
    }
    let mut out: Vec<usize> = (0..n).collect();
    for (i, u) in (0..size).zip(uniforms(seed)) {
        let remaining = n - i;
        let j = i + ((u * remaining as f64) as usize).min(remaining - 1);
        out.swap(i, j);
    }
    out.truncate(size);
    Ok(out)
}
