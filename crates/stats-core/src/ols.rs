use crate::{ErrorKind, StatsError};
use nalgebra::{DMatrix, DVector};
use statrs::distribution::{ContinuousCDF, StudentsT};

#[derive(Debug, Clone)]
pub struct OlsResult {
    pub coefficients: Vec<f64>,
    pub covariance: Vec<Vec<f64>>,
    pub std_errors: Vec<f64>,
    pub t_statistics: Vec<f64>,
    pub p_values: Vec<f64>,
    pub conf_int: Vec<[f64; 2]>,
    pub fitted: Vec<f64>,
    pub residuals: Vec<f64>,
    pub rss: f64,
    pub residual_variance: f64,
    pub sigma: f64,
    pub r_squared: f64,
    pub adj_r_squared: f64,
    pub df_residual: usize,
    pub log_likelihood: f64,
}

fn error(kind: ErrorKind, message: &str) -> StatsError {
    StatsError::new(kind, message)
}

// Solve P(T > critical) = 0.025 directly. statrs' inverse-beta quantile loses
// precision for large residual df, where its beta argument is very close to 1.
// Bisection on the survival function avoids that inverse-beta stopping error.
pub(crate) fn t_critical_95(distribution: &StudentsT) -> f64 {
    let mut lower = 0.0;
    let mut upper = 1.0;
    while distribution.sf(upper) > 0.025 {
        upper *= 2.0;
    }
    for _ in 0..64 {
        let midpoint = (lower + upper) / 2.0;
        if distribution.sf(midpoint) > 0.025 {
            lower = midpoint;
        } else {
            upper = midpoint;
        }
    }
    (lower + upper) / 2.0
}

/// Fit a full-rank design; an intercept must be supplied as an exact column of ones.
pub fn fit_ols(x: Vec<Vec<f64>>, y: Vec<f64>) -> Result<OlsResult, StatsError> {
    let n = x.len();
    if n == 0 || x[0].is_empty() {
        return Err(error(
            ErrorKind::InvalidInput,
            "x must have at least one row and column",
        ));
    }
    let p = x[0].len();
    if y.len() != n {
        return Err(error(
            ErrorKind::DimensionMismatch,
            "x and y must have the same number of observations",
        ));
    }
    if x.iter().any(|row| row.len() != p) {
        return Err(error(ErrorKind::DimensionMismatch, "x must be rectangular"));
    }
    if x.iter().flatten().chain(y.iter()).any(|v| !v.is_finite()) {
        return Err(error(
            ErrorKind::Nonfinite,
            "x and y must contain only finite numbers",
        ));
    }
    if n <= p {
        return Err(error(
            ErrorKind::DimensionMismatch,
            "OLS requires n > p for residual degrees of freedom",
        ));
    }
    let intercept = (0..p).any(|j| x.iter().all(|row| row[j] == 1.0));
    let design = DMatrix::from_row_iterator(n, p, x.into_iter().flatten());
    let outcomes = DVector::from_vec(y);
    // Scale columns before SVD so rank detection is invariant to predictor units.
    let scales: Vec<f64> = (0..p).map(|j| design.column(j).norm()).collect();
    if scales.iter().any(|s| !s.is_finite() || *s == 0.0) {
        return Err(error(
            ErrorKind::RankDeficiency,
            "design is rank deficient or its scale exceeds floating-point range",
        ));
    }
    let scaled = DMatrix::from_fn(n, p, |i, j| design[(i, j)] / scales[j]);
    // Householder QR of the scaled design. R's singular values (those of the
    // design) drive the rank check; coefficients and covariance come from R by
    // back-substitution, never from singular vectors or X'X.
    let qr = scaled.qr();
    let mut qty = outcomes.clone();
    qr.q_tr_mul(&mut qty);
    let r = qr.r();
    let singular = r.singular_values();
    let tolerance = f64::EPSILON * n.max(p) as f64 * singular.max();
    if singular.iter().any(|s| *s <= tolerance) {
        return Err(error(
            ErrorKind::RankDeficiency,
            "design is rank deficient at numerical precision",
        ));
    }
    let beta_scaled = r
        .solve_upper_triangular(&qty.rows(0, p).into_owned())
        .ok_or_else(|| {
            error(
                ErrorKind::FactorizationFailure,
                "least-squares factorization failed",
            )
        })?;
    let beta = DVector::from_iterator(p, (0..p).map(|j| beta_scaled[j] / scales[j]));
    let fitted = &design * &beta;
    let residuals = &outcomes - &fitted;
    // Preserve actual floating-point residuals, including nearly perfect fits.
    let rss = residuals.norm_squared();
    let df = n - p;
    let variance = rss / df as f64;
    if (rss == 0.0 && residuals.iter().any(|v| *v != 0.0)) || (rss > 0.0 && variance == 0.0) {
        return Err(error(
            ErrorKind::Nonfinite,
            "nonzero residual variance underflows floating-point range; rescale the outcome",
        ));
    }
    // (X'X)^-1 = D^-1 V diag(1/s^2) V' D^-1; never form X'X.
    let gram_inverse = crate::observations::inverse_gram(&r).ok_or_else(|| {
        error(
            ErrorKind::FactorizationFailure,
            "triangular factor could not be inverted",
        )
    })?;
    let covariance = DMatrix::from_fn(p, p, |i, j| {
        gram_inverse[(i, j)] / scales[i] / scales[j] * variance
    });
    let distribution = StudentsT::new(0.0, 1.0, df as f64)
        .map_err(|_| error(ErrorKind::Internal, "invalid t distribution"))?;
    let critical = t_critical_95(&distribution);
    let std_errors: Vec<f64> = (0..p).map(|j| covariance[(j, j)].sqrt()).collect();
    let t_statistics: Vec<f64> = (0..p).map(|j| beta[j] / std_errors[j]).collect();
    // Survival function avoids cancellation in extreme tails.
    let p_values = t_statistics
        .iter()
        .map(|t| {
            if t.is_nan() {
                f64::NAN
            } else if t.is_infinite() {
                0.0
            } else {
                2.0 * distribution.sf(t.abs())
            }
        })
        .collect();
    let conf_int = (0..p)
        .map(|j| {
            [
                beta[j] - critical * std_errors[j],
                beta[j] + critical * std_errors[j],
            ]
        })
        .collect();
    let mean = if intercept {
        outcomes.iter().sum::<f64>() / n as f64
    } else {
        0.0
    };
    let tss = outcomes.iter().map(|v| (v - mean).powi(2)).sum::<f64>();
    if !tss.is_finite() {
        return Err(error(
            ErrorKind::Nonfinite,
            "total outcome variation exceeds floating-point range; rescale the outcome",
        ));
    }
    let r_squared = if p == 1 && intercept {
        0.0
    } else {
        1.0 - rss / tss
    };
    let adj_r_squared = if p == 1 && intercept {
        0.0
    } else {
        1.0 - (1.0 - r_squared) * (n - usize::from(intercept)) as f64 / df as f64
    };
    // Gaussian likelihood uses ML variance RSS/n, unlike inference variance RSS/(n-p).
    let log_likelihood =
        -0.5 * n as f64 * ((2.0 * std::f64::consts::PI).ln() + (rss / n as f64).ln() + 1.0);
    if beta
        .iter()
        .chain(fitted.iter())
        .chain(residuals.iter())
        .any(|v| !v.is_finite())
        || !rss.is_finite()
        || covariance.iter().any(|v| !v.is_finite())
    {
        return Err(error(
            ErrorKind::Nonfinite,
            "calculation exceeds floating-point range",
        ));
    }
    Ok(OlsResult {
        coefficients: beta.iter().copied().collect(),
        covariance: (0..p)
            .map(|i| (0..p).map(|j| covariance[(i, j)]).collect())
            .collect(),
        std_errors,
        t_statistics,
        p_values,
        conf_int,
        fitted: fitted.iter().copied().collect(),
        residuals: residuals.iter().copied().collect(),
        rss,
        residual_variance: variance,
        sigma: variance.sqrt(),
        r_squared,
        adj_r_squared,
        df_residual: df,
        log_likelihood,
    })
}

/// Weighted Gaussian fit with known additive offsets. Prior weights specify
/// Var(e_i)=sigma²/w_i; retain response residuals, not whitened residuals.
pub fn fit_ols_with_data(
    x: Vec<Vec<f64>>,
    y: Vec<f64>,
    weights: Vec<f64>,
    offset: Vec<f64>,
) -> Result<OlsResult, StatsError> {
    let n = x.len();
    let (weights, offset) = crate::observations::prepare(n, weights, offset, false)?;
    if y.len() != n
        || n == 0
        || x[0].is_empty()
        || x.iter().any(|r| r.len() != x[0].len())
        || y.iter().any(|v| !v.is_finite())
        || x.iter().flatten().any(|v| !v.is_finite())
    {
        return Err(error(
            ErrorKind::Nonfinite,
            "x and y must be finite, rectangular and aligned",
        ));
    }
    // Preserve the existing unweighted behavior and its edge-case conventions.
    if weights.iter().all(|v| *v == 1.0) && offset.iter().all(|v| *v == 0.0) {
        return fit_ols(x, y);
    }
    let p = x[0].len();
    let intercept = (0..p).any(|j| x.iter().all(|r| r[j] == 1.0));
    // Householder QR is unstable when row scales differ by many orders of
    // magnitude unless the heavy rows come first (Powell & Reid), so factor the
    // whitened rows in decreasing-weight order. fitted/residuals are rebuilt in
    // the original order below; the other results do not depend on row order.
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by(|a, b| weights[*b].total_cmp(&weights[*a]));
    let mut result = fit_ols(
        order
            .iter()
            .map(|&i| x[i].iter().map(|v| v * weights[i].sqrt()).collect())
            .collect(),
        order
            .iter()
            .map(|&i| (y[i] - offset[i]) * weights[i].sqrt())
            .collect(),
    )?;
    result.fitted = (0..n)
        .map(|i| {
            offset[i]
                + x[i]
                    .iter()
                    .zip(&result.coefficients)
                    .map(|(x, b)| x * b)
                    .sum::<f64>()
        })
        .collect();
    result.residuals = y.iter().zip(&result.fitted).map(|(y, f)| y - f).collect();
    // R summary.lm removes the offset from fitted values before calculating MSS.
    let fixed: Vec<f64> = result
        .fitted
        .iter()
        .zip(&offset)
        .map(|(f, o)| f - o)
        .collect();
    let mean = if intercept {
        fixed.iter().zip(&weights).map(|(f, w)| f * w).sum::<f64>() / weights.iter().sum::<f64>()
    } else {
        0.0
    };
    let mss = fixed
        .iter()
        .zip(&weights)
        .map(|(f, w)| w * (f - mean).powi(2))
        .sum::<f64>();
    if !mss.is_finite() {
        return Err(error(
            ErrorKind::Nonfinite,
            "weighted fitted variation exceeds floating-point range; rescale the outcome",
        ));
    }
    result.r_squared = if p == 1 && intercept {
        0.0
    } else {
        mss / (mss + result.rss)
    };
    result.adj_r_squared = if p == 1 && intercept {
        0.0
    } else {
        1.0 - (1.0 - result.r_squared) * (n - usize::from(intercept)) as f64 / (n - p) as f64
    };
    result.log_likelihood += 0.5 * weights.iter().map(|w| w.ln()).sum::<f64>();
    if result
        .fitted
        .iter()
        .chain(result.residuals.iter())
        .any(|v| !v.is_finite())
    {
        return Err(error(
            ErrorKind::Nonfinite,
            "unweighted response-scale results exceed floating-point range",
        ));
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn confidence_critical_values_agree_with_r_at_large_df() {
        // R 4.6.1: qt(0.975, df). Large df exposed inverse-beta inaccuracy.
        for (df, expected) in [
            (1., 12.706204736174692),
            (3., 3.1824463052837078),
            (30., 2.0422724563012378),
            (997., 1.9623462360894495),
            (99997., 1.9599877082463284),
            (999997., 1.9599663568212229),
        ] {
            let distribution = StudentsT::new(0., 1., df).unwrap();
            let actual = t_critical_95(&distribution);
            assert!(
                (actual - expected).abs() < 1e-8,
                "df={df}: {actual} != {expected}"
            );
        }
    }

    #[test]
    fn zero_outcomes_have_explicit_undefined_inference() {
        let result = fit_ols(vec![vec![1.], vec![1.], vec![1.]], vec![0.; 3]).unwrap();
        assert_eq!(result.coefficients, vec![0.]);
        assert_eq!(result.std_errors, vec![0.]);
        assert!(result.t_statistics[0].is_nan());
        assert!(result.p_values[0].is_nan());
        assert_eq!(result.conf_int, vec![[0., 0.]]);
        assert_eq!(result.r_squared, 0.0);
        assert_eq!(result.log_likelihood, f64::INFINITY);
    }
}
