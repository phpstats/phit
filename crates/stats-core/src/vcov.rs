//! Sandwich covariance from published estimating-equation formulas; no reference source.
use crate::{ErrorKind, OlsResult, StatsError};
use nalgebra::DMatrix;
use statrs::distribution::{ContinuousCDF, StudentsT};

#[derive(Clone, Debug)]
pub enum Vcov {
    Hc(u8),
    Cluster { groups: Vec<usize>, hc1: bool },
    NeweyWest { lag: usize, adjust: bool },
}
fn invalid(message: &str) -> StatsError {
    StatsError::new(ErrorKind::InvalidInput, message)
}

/// Column-scaled QR, returning Q and the inverse Gram in original column units.
fn geometry(x: &DMatrix<f64>) -> Result<(DMatrix<f64>, DMatrix<f64>), StatsError> {
    let p = x.ncols();
    let scales: Vec<_> = (0..p).map(|j| x.column(j).norm()).collect();
    if scales.iter().any(|s| !s.is_finite() || *s == 0.) {
        return Err(invalid("covariance design has a zero or nonfinite column"));
    }
    let qr = DMatrix::from_fn(x.nrows(), p, |i, j| x[(i, j)] / scales[j]).qr();
    let r = qr.r();
    let singular = r.singular_values();
    let tolerance = f64::EPSILON * x.nrows().max(p) as f64 * singular.max();
    if singular.len() != p || singular.iter().any(|s| *s <= tolerance) {
        return Err(StatsError::new(
            ErrorKind::RankDeficiency,
            "covariance design is rank deficient",
        ));
    }
    let inverse = crate::observations::inverse_gram(&r)
        .ok_or_else(|| invalid("covariance QR inverse failed"))?;
    Ok((
        qr.q(),
        DMatrix::from_fn(p, p, |i, j| inverse[(i, j)] / scales[i] / scales[j]),
    ))
}

/// Structural residuals, projected regressors for IV, and weighted estimating scores.
/// Returns covariance and the degrees of freedom used for coefficient inference.
pub fn covariance(
    x: &[Vec<f64>],
    z: Option<&[Vec<f64>]>,
    weights: &[f64],
    residuals: &[f64],
    method: &Vcov,
) -> Result<(Vec<Vec<f64>>, usize), StatsError> {
    let n = x.len();
    let p = x.first().map_or(0, Vec::len);
    if p == 0
        || n <= p
        || weights.len() != n
        || residuals.len() != n
        || x.iter().any(|r| r.len() != p)
    {
        return Err(invalid(
            "covariance inputs must align and have positive residual df",
        ));
    }
    if x.iter()
        .flatten()
        .chain(weights)
        .chain(residuals)
        .any(|v| !v.is_finite())
        || weights.iter().any(|w| *w <= 0.)
    {
        return Err(invalid(
            "covariance inputs must be finite with positive weights",
        ));
    }
    let max_weight = weights.iter().copied().fold(0., f64::max);
    let root: Vec<_> = weights.iter().map(|w| (w / max_weight).sqrt()).collect();
    let mut design = DMatrix::from_fn(n, p, |i, j| root[i] * x[i][j]);
    if let Some(z) = z {
        let l = z.first().map_or(0, Vec::len);
        if l < p
            || z.len() != n
            || z.iter()
                .any(|r| r.len() != l || r.iter().any(|v| !v.is_finite()))
        {
            return Err(invalid("instrument covariance design must align"));
        }
        let wz = DMatrix::from_fn(n, l, |i, j| root[i] * z[i][j]);
        let (q, _) = geometry(&wz)?;
        design = &q * (q.transpose() * design);
    }
    let (q, bread) = geometry(&design)?;
    let mut influence = &design * bread;
    for i in 0..n {
        influence.row_mut(i).scale_mut(root[i] * residuals[i]);
    }
    let mut inference_df = n - p;
    let mut covariance = match method {
        Vcov::Hc(kind) => {
            if *kind > 3 {
                return Err(invalid("HC type must be HC0, HC1, HC2 or HC3"));
            }
            for i in 0..n {
                let h = q.row(i).norm_squared();
                let factor = match kind {
                    0 => 1.,
                    1 => (n as f64 / (n - p) as f64).sqrt(),
                    _ => {
                        if 1. - h <= f64::EPSILON * 16. {
                            return Err(StatsError::new(
                                ErrorKind::FactorizationFailure,
                                "HC2/HC3 undefined at unit leverage",
                            ));
                        }
                        if *kind == 2 {
                            (1. - h).sqrt().recip()
                        } else {
                            (1. - h).recip()
                        }
                    }
                };
                influence.row_mut(i).scale_mut(factor);
            }
            influence.transpose() * influence
        }
        Vcov::Cluster { groups, hc1 } => {
            if groups.len() != n {
                return Err(invalid("cluster labels must align with retained rows"));
            }
            let mut labels = groups.clone();
            labels.sort_unstable();
            labels.dedup();
            let g = labels.len();
            if g < 2 {
                return Err(invalid("cluster covariance requires at least two clusters"));
            }
            inference_df = g - 1;
            let mut summed = DMatrix::<f64>::zeros(g, p);
            for i in 0..n {
                let row = labels.binary_search(&groups[i]).unwrap();
                for j in 0..p {
                    summed[(row, j)] += influence[(i, j)];
                }
            }
            let correction = g as f64 / (g - 1) as f64
                * if *hc1 {
                    (n - 1) as f64 / (n - p) as f64
                } else {
                    1.
                };
            (summed.transpose() * summed) * correction
        }
        Vcov::NeweyWest { lag, adjust } => {
            if *lag >= n {
                return Err(invalid(
                    "Newey-West lag must be less than the retained row count",
                ));
            }
            let mut cov = influence.transpose() * &influence;
            for lag_i in 1..=*lag {
                let factor = 1. - lag_i as f64 / (*lag + 1) as f64;
                for i in lag_i..n {
                    for j in 0..p {
                        for k in 0..p {
                            cov[(j, k)] += factor
                                * (influence[(i, j)] * influence[(i - lag_i, k)]
                                    + influence[(i - lag_i, j)] * influence[(i, k)]);
                        }
                    }
                }
            }
            if *adjust {
                cov *= n as f64 / (n - p) as f64;
            }
            cov
        }
    };
    // Roundoff can introduce an antisymmetric component; covariance is symmetric.
    covariance = (&covariance + covariance.transpose()) * 0.5;
    if covariance.iter().any(|v| !v.is_finite()) || (0..p).any(|j| covariance[(j, j)] < 0.) {
        return Err(StatsError::new(
            ErrorKind::FactorizationFailure,
            "covariance is nonfinite or has negative variance",
        ));
    }
    Ok((
        (0..p)
            .map(|i| (0..p).map(|j| covariance[(i, j)]).collect())
            .collect(),
        inference_df,
    ))
}

pub fn replace_inference(fit: &mut OlsResult, covariance: Vec<Vec<f64>>, df: usize) {
    let t = StudentsT::new(0., 1., df as f64).unwrap();
    let critical = crate::ols::t_critical_95(&t);
    fit.std_errors = (0..fit.coefficients.len())
        .map(|j| covariance[j][j].sqrt())
        .collect();
    fit.t_statistics = fit
        .coefficients
        .iter()
        .zip(&fit.std_errors)
        .map(|(b, se)| b / se)
        .collect();
    fit.p_values = fit
        .t_statistics
        .iter()
        .map(|v| 2. * t.sf(v.abs()))
        .collect();
    fit.conf_int = fit
        .coefficients
        .iter()
        .zip(&fit.std_errors)
        .map(|(b, se)| [b - critical * se, b + critical * se])
        .collect();
    fit.covariance = covariance;
}
