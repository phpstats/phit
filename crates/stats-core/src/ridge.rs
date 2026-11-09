//! Gaussian ridge and grouped cross-validation. Clean-room conventions: specs/cleanroom/ridge.md.
use crate::{ErrorKind, StatsError};
use nalgebra::{DMatrix, DVector};

#[derive(Clone, Debug)]
pub struct RidgeOptions {
    pub lambda: f64,
    pub penalty_factor: Vec<f64>,
    pub standardize: bool,
    pub bounds: Vec<[Option<f64>; 2]>,
    pub intercept: Option<usize>,
}
#[derive(Clone, Debug)]
pub struct RidgeResult {
    pub coefficients: Vec<f64>,
    pub covariance: Vec<Vec<Option<f64>>>,
    pub std_errors: Vec<Option<f64>>,
    pub conf_int: Vec<Option<[f64; 2]>>,
    pub fitted: Vec<f64>,
    pub residuals: Vec<f64>,
    pub rss: f64,
    pub sigma: f64,
    pub df: f64,
    pub df_residual: f64,
    pub r_squared: f64,
    pub penalty_factor: Vec<f64>,
    pub at_bound: Vec<bool>,
    pub lambda_effective: f64,
    pub response_scale: f64,
    pub lambda_lm_ridge: Option<f64>,
    pub iterations: usize,
}
#[derive(Clone, Debug)]
pub struct CvResult {
    pub lambda: Vec<f64>,
    pub cvm: Vec<f64>,
    pub cvsd: Vec<f64>,
    pub index_min: usize,
    pub index_1se: usize,
    pub fit: RidgeResult,
}
fn error(kind: ErrorKind, message: &str) -> StatsError {
    StatsError::new(kind, message)
}
fn invalid(message: &str) -> StatsError {
    error(ErrorKind::InvalidInput, message)
}

/// Shell-style * and ? patterns; all other characters are literal design-name characters.
pub fn glob_matches(pattern: &str, name: &str) -> bool {
    let chars: Vec<_> = name.chars().collect();
    let mut previous = vec![false; chars.len() + 1];
    previous[0] = true;
    for c in pattern.chars() {
        let mut next = vec![false; chars.len() + 1];
        if c == '*' {
            next[0] = previous[0];
        }
        for j in 1..=chars.len() {
            next[j] = if c == '*' {
                previous[j] || next[j - 1]
            } else {
                previous[j - 1] && (c == '?' || c == chars[j - 1])
            };
        }
        previous = next;
    }
    previous[chars.len()]
}
/// Column-scaled augmented QR, for both coefficients and inverse penalized Gram.
fn solve(a: &DMatrix<f64>, y: &DVector<f64>) -> Result<(DVector<f64>, DMatrix<f64>), StatsError> {
    let p = a.ncols();
    if p == 0 {
        return Ok((DVector::zeros(0), DMatrix::zeros(0, 0)));
    }
    let scales: Vec<_> = (0..p).map(|j| a.column(j).norm()).collect();
    if scales.iter().any(|s| !s.is_finite() || *s == 0.) {
        return Err(error(
            ErrorKind::RankDeficiency,
            "unpenalized ridge design is rank deficient",
        ));
    }
    let qr = DMatrix::from_fn(a.nrows(), p, |i, j| a[(i, j)] / scales[j]).qr();
    let r = qr.r();
    let singular = r.singular_values();
    let tolerance = f64::EPSILON * a.nrows().max(p) as f64 * singular.max();
    if singular.len() != p || singular.iter().any(|s| *s <= tolerance) {
        return Err(error(
            ErrorKind::RankDeficiency,
            "unpenalized ridge design is rank deficient at numerical precision",
        ));
    }
    let mut qty = y.clone();
    qr.q_tr_mul(&mut qty);
    let b = r
        .solve_upper_triangular(&qty.rows(0, p).into_owned())
        .ok_or_else(|| error(ErrorKind::FactorizationFailure, "ridge QR solve failed"))?;
    let inverse = crate::observations::inverse_gram(&r)
        .ok_or_else(|| error(ErrorKind::FactorizationFailure, "ridge QR inverse failed"))?;
    Ok((
        DVector::from_iterator(p, (0..p).map(|j| b[j] / scales[j])),
        DMatrix::from_fn(p, p, |i, j| inverse[(i, j)] / scales[i] / scales[j]),
    ))
}

pub fn fit_ridge(
    x: &[Vec<f64>],
    y: &[f64],
    weights: &[f64],
    options: &RidgeOptions,
) -> Result<RidgeResult, StatsError> {
    let n = x.len();
    let p = x.first().map_or(0, Vec::len);
    if n == 0
        || p == 0
        || y.len() != n
        || weights.len() != n
        || x.iter().any(|r| r.len() != p)
        || options.penalty_factor.len() != p
        || options.bounds.len() != p
        || options.intercept.is_some_and(|j| j >= p)
    {
        return Err(error(
            ErrorKind::DimensionMismatch,
            "ridge design, response, weights, and column options must align",
        ));
    }
    if x.iter()
        .flatten()
        .chain(y)
        .chain(weights)
        .any(|v| !v.is_finite())
        || weights.iter().any(|w| *w <= 0.)
    {
        return Err(invalid("ridge data must be finite and weights positive"));
    }
    if !options.lambda.is_finite()
        || options.lambda < 0.
        || options
            .penalty_factor
            .iter()
            .any(|v| !v.is_finite() || *v < 0.)
    {
        return Err(invalid(
            "lambda and penalty factors must be finite and nonnegative",
        ));
    }
    if options
        .bounds
        .iter()
        .any(|b| b.iter().flatten().any(|v| !v.is_finite()) || matches!(b,[Some(a),Some(z)] if a>z))
    {
        return Err(invalid(
            "bounds must be finite or null, with lower <= upper",
        ));
    }
    if let Some(j) = options.intercept
        && (x.iter().any(|r| r[j] != 1.)
            || options.penalty_factor[j] != 0.
            || options.bounds[j] != [None, None])
    {
        return Err(invalid(
            "the intercept must be a column of ones, unpenalized and unbounded",
        ));
    }
    let weight_max = weights.iter().copied().fold(0., f64::max);
    let weight_sum = weights.iter().map(|w| w / weight_max).sum::<f64>();
    let q: Vec<_> = weights
        .iter()
        .map(|w| (w / weight_max) / weight_sum)
        .collect();
    let centers: Vec<_> = (0..p)
        .map(|j| {
            if Some(j) == options.intercept {
                0.
            } else {
                (0..n).map(|i| q[i] * x[i][j]).sum::<f64>()
            }
        })
        .collect();
    let means: Vec<_> = (0..p)
        .map(|j| {
            if options.intercept.is_some() {
                centers[j]
            } else {
                0.
            }
        })
        .collect();
    let ymean = if options.intercept.is_some() {
        (0..n).map(|i| q[i] * y[i]).sum::<f64>()
    } else {
        0.
    };
    let response_scale = (0..n)
        .map(|i| q[i] * (y[i] - ymean).powi(2))
        .sum::<f64>()
        .sqrt();
    if !response_scale.is_finite() || response_scale == 0. {
        return Err(invalid(
            "ridge requires nonzero finite response variation (RMS without an intercept)",
        ));
    }
    let effective = options.lambda / response_scale;
    let variances: Vec<_> = (0..p)
        .map(|j| {
            (0..n)
                .map(|i| q[i] * (x[i][j] - centers[j]).powi(2))
                .sum::<f64>()
        })
        .collect();
    let constant: Vec<_> = (0..p)
        .map(|j| {
            Some(j) != options.intercept
                && (x.iter().all(|r| r[j] == x[0][j]) || variances[j] == 0.)
        })
        .collect();
    let scales: Vec<_> = (0..p)
        .map(|j| {
            if options.standardize && Some(j) != options.intercept && !constant[j] {
                variances[j].sqrt()
            } else {
                1.
            }
        })
        .collect();
    let predictors = p - usize::from(options.intercept.is_some());
    let factor_sum = options.penalty_factor.iter().sum::<f64>();
    let factors: Vec<_> = options
        .penalty_factor
        .iter()
        .map(|v| {
            if factor_sum > 0. {
                *v * predictors as f64 / factor_sum
            } else {
                0.
            }
        })
        .collect();
    let design = DMatrix::from_fn(n, p, |i, j| {
        if constant[j] {
            0.
        } else {
            (x[i][j] - means[j]) / scales[j]
        }
    });
    let mut augmented = DMatrix::zeros(n + p, p);
    for i in 0..n {
        for j in 0..p {
            augmented[(i, j)] = q[i].sqrt() * design[(i, j)];
        }
    }
    for j in 0..p {
        augmented[(n + j, j)] = (effective * factors[j]).sqrt();
    }
    let mut outcome = DVector::zeros(n + p);
    for i in 0..n {
        outcome[i] = q[i].sqrt() * (y[i] - ymean);
    }
    let lower: Vec<_> = (0..p)
        .map(|j| options.bounds[j][0].unwrap_or(f64::NEG_INFINITY) * scales[j])
        .collect();
    let upper: Vec<_> = (0..p)
        .map(|j| options.bounds[j][1].unwrap_or(f64::INFINITY) * scales[j])
        .collect();
    if (0..p).any(|j| constant[j] && !(lower[j] <= 0. && upper[j] >= 0.)) {
        return Err(invalid("bounds for constant predictors must include zero"));
    }
    let available: Vec<_> = (0..p)
        .filter(|j| !constant[*j] && lower[*j] != upper[*j])
        .collect();
    let a = DMatrix::from_fn(n + p, available.len(), |i, j| augmented[(i, available[j])]);
    let mut theta = DVector::zeros(p);
    for j in 0..p {
        if lower[j] == upper[j] {
            theta[j] = lower[j];
        }
    }
    let adjusted = &outcome - &augmented * &theta;
    let (initial, initial_inverse) = solve(&a, &adjusted)?;
    for (j, index) in available.iter().enumerate() {
        theta[*index] = initial[j].clamp(lower[*index], upper[*index]);
    }
    let bounded = options.bounds.iter().any(|b| *b != [None, None]);
    let mut iterations = 0;
    if bounded {
        let mut residual = &outcome - &augmented * &theta;
        let diagonals: Vec<_> = (0..p).map(|j| augmented.column(j).norm_squared()).collect();
        let mut converged = false;
        for iteration in 0..100_000 {
            for &j in &available {
                let next = (theta[j] + augmented.column(j).dot(&residual) / diagonals[j])
                    .clamp(lower[j], upper[j]);
                let change = next - theta[j];
                theta[j] = next;
                if change != 0. {
                    residual -= augmented.column(j) * change;
                }
            }
            iterations = iteration + 1;
            let mut violation = 0_f64;
            for &j in &available {
                let gradient = -augmented.column(j).dot(&residual);
                let g = if theta[j] <= lower[j] {
                    gradient.min(0.)
                } else if theta[j] >= upper[j] {
                    gradient.max(0.)
                } else {
                    gradient
                };
                violation = violation.max(g.abs() / diagonals[j].sqrt());
            }
            if violation <= 1e-12 * (1. + outcome.norm()) {
                converged = true;
                break;
            }
        }
        if !converged {
            return Err(error(
                ErrorKind::Nonconvergence,
                "bounded ridge did not satisfy its KKT tolerance",
            ));
        }
    }
    let at_bound: Vec<_> = (0..p)
        .map(|j| !constant[j] && (theta[j] == lower[j] || theta[j] == upper[j]))
        .collect();
    let free: Vec<_> = (0..p).filter(|j| !constant[*j] && !at_bound[*j]).collect();
    let a_free = DMatrix::from_fn(n + p, free.len(), |i, j| augmented[(i, free[j])]);
    let inverse = if free == available {
        initial_inverse
    } else {
        solve(&a_free, &outcome)?.1
    };
    let d_free = DMatrix::from_fn(n, free.len(), |i, j| q[i].sqrt() * design[(i, free[j])]);
    let gram = d_free.transpose() * d_free;
    let df = (&inverse * &gram).trace();
    let df_residual = n as f64 - df;
    if df_residual <= f64::EPSILON * n as f64 {
        return Err(error(
            ErrorKind::DimensionMismatch,
            "ridge requires positive effective residual degrees of freedom",
        ));
    }
    let mut transform = DMatrix::zeros(p, free.len());
    for (h, &j) in free.iter().enumerate() {
        transform[(j, h)] = 1. / scales[j];
        if let Some(intercept) = options.intercept
            && j != intercept
        {
            transform[(intercept, h)] = -means[j] / scales[j];
        }
    }
    let mut beta = DVector::from_iterator(p, (0..p).map(|j| theta[j] / scales[j]));
    if let Some(j) = options.intercept {
        beta[j] += ymean
            - (0..p)
                .filter(|h| *h != j)
                .map(|h| means[h] * beta[h])
                .sum::<f64>();
    }
    let fitted: Vec<_> = x
        .iter()
        .map(|r| r.iter().zip(beta.iter()).map(|(x, b)| x * b).sum::<f64>())
        .collect();
    let residuals: Vec<_> = y.iter().zip(&fitted).map(|(y, f)| y - f).collect();
    let rss = (0..n)
        .map(|i| q[i] * n as f64 * residuals[i].powi(2))
        .sum::<f64>();
    let variance = rss / df_residual;
    let covariance = &transform
        * (&inverse * &gram * &inverse)
        * (transform.transpose())
        * (variance / n as f64);
    let t = crate::distributions::StudentT::new(df_residual)?;
    let critical = t.quantile(0.975);
    let std_errors: Vec<_> = (0..p)
        .map(|j| (!at_bound[j]).then(|| covariance[(j, j)].max(0.).sqrt()))
        .collect();
    let conf_int = (0..p)
        .map(|j| std_errors[j].map(|s| [beta[j] - critical * s, beta[j] + critical * s]))
        .collect();
    let covariance = (0..p)
        .map(|i| {
            (0..p)
                .map(|j| (!at_bound[i] && !at_bound[j]).then_some(covariance[(i, j)]))
                .collect()
        })
        .collect();
    let tss = n as f64 * response_scale.powi(2);
    let convertible = options.standardize
        && options.intercept.is_some()
        && !bounded
        && weights.iter().all(|w| *w == weights[0])
        && (0..p).all(|j| Some(j) == options.intercept || factors[j] == 1.);
    if beta
        .iter()
        .chain(&fitted)
        .chain(&residuals)
        .any(|v| !v.is_finite())
        || !rss.is_finite()
        || !variance.is_finite()
    {
        return Err(error(
            ErrorKind::Nonfinite,
            "ridge results exceed floating-point range",
        ));
    }
    Ok(RidgeResult {
        coefficients: beta.iter().copied().collect(),
        covariance,
        std_errors,
        conf_int,
        fitted,
        residuals,
        rss,
        sigma: variance.sqrt(),
        df,
        df_residual,
        r_squared: 1. - rss / tss,
        penalty_factor: factors,
        at_bound,
        lambda_effective: effective,
        response_scale,
        lambda_lm_ridge: convertible.then_some(n as f64 * effective),
        iterations,
    })
}

/// phit path: 100 decreasing log-spaced values over 1e4, using training weighted scales.
pub fn default_lambda(
    x: &[Vec<f64>],
    y: &[f64],
    weights: &[f64],
    options: &RidgeOptions,
) -> Result<Vec<f64>, StatsError> {
    let fit = fit_ridge(
        x,
        y,
        weights,
        &RidgeOptions {
            lambda: 1.,
            ..options.clone()
        },
    )?;
    let upper = fit.response_scale.max(f64::MIN_POSITIVE) * 100.;
    Ok((0..100)
        .map(|j| upper * 10_f64.powf(-4. * j as f64 / 99.))
        .collect())
}

pub fn cv_ridge(
    x: &[Vec<f64>],
    y: &[f64],
    weights: &[f64],
    options: &RidgeOptions,
    lambda: &[f64],
    foldid: &[usize],
) -> Result<CvResult, StatsError> {
    if lambda.is_empty()
        || lambda.iter().any(|l| !l.is_finite() || *l < 0.)
        || lambda.windows(2).any(|w| w[0] <= w[1])
    {
        return Err(invalid(
            "lambda must be a nonempty strictly decreasing sequence of finite nonnegative values",
        ));
    }
    if foldid.len() != x.len() {
        return Err(error(
            ErrorKind::DimensionMismatch,
            "foldid must have one label per retained row",
        ));
    }
    let mut folds = foldid.to_vec();
    folds.sort_unstable();
    folds.dedup();
    if folds.len() < 3 {
        return Err(invalid("cv_ridge requires at least three nonempty folds"));
    }
    let mut losses = Vec::new();
    let mut fold_weights = Vec::new();
    let grouped = x.len() / folds.len() >= 3;
    for fold in &folds {
        let train: Vec<_> = (0..x.len()).filter(|i| foldid[*i] != *fold).collect();
        let test: Vec<_> = (0..x.len()).filter(|i| foldid[*i] == *fold).collect();
        let tx: Vec<_> = train.iter().map(|i| x[*i].clone()).collect();
        let ty: Vec<_> = train.iter().map(|i| y[*i]).collect();
        let tw: Vec<_> = train.iter().map(|i| weights[*i]).collect();
        let sum = test.iter().map(|i| weights[*i]).sum::<f64>();
        let mut fold_loss = Vec::new();
        let mut observation_losses = vec![Vec::new(); test.len()];
        for l in lambda {
            let fit = fit_ridge(
                &tx,
                &ty,
                &tw,
                &RidgeOptions {
                    lambda: *l,
                    ..options.clone()
                },
            )?;
            let mut error = 0.;
            for (j, &i) in test.iter().enumerate() {
                let predicted = x[i]
                    .iter()
                    .zip(&fit.coefficients)
                    .map(|(x, b)| x * b)
                    .sum::<f64>();
                let loss = (y[i] - predicted).powi(2);
                error += weights[i] * loss;
                observation_losses[j].push(loss);
            }
            fold_loss.push(error / sum);
        }
        if grouped {
            losses.push(fold_loss);
            fold_weights.push(sum);
        } else {
            for (j, &i) in test.iter().enumerate() {
                losses.push(observation_losses[j].clone());
                fold_weights.push(weights[i]);
            }
        }
    }
    let sum = fold_weights.iter().sum::<f64>();
    let cvm: Vec<_> = (0..lambda.len())
        .map(|j| {
            losses
                .iter()
                .zip(&fold_weights)
                .map(|(loss, w)| w * loss[j])
                .sum::<f64>()
                / sum
        })
        .collect();
    let cvsd: Vec<_> = (0..lambda.len())
        .map(|j| {
            (losses
                .iter()
                .zip(&fold_weights)
                .map(|(loss, w)| w * (loss[j] - cvm[j]).powi(2))
                .sum::<f64>()
                / sum
                / (losses.len() - 1) as f64)
                .sqrt()
        })
        .collect();
    let index_min = (0..lambda.len())
        .min_by(|a, b| cvm[*a].total_cmp(&cvm[*b]))
        .unwrap();
    let index_1se = (0..lambda.len())
        .find(|j| cvm[*j] <= cvm[index_min] + cvsd[index_min])
        .unwrap();
    let fit = fit_ridge(
        x,
        y,
        weights,
        &RidgeOptions {
            lambda: lambda[index_min],
            ..options.clone()
        },
    )?;
    Ok(CvResult {
        lambda: lambda.to_vec(),
        cvm,
        cvsd,
        index_min,
        index_1se,
        fit,
    })
}
