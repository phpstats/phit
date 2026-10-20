//! Binomial logit with one Gaussian random intercept: marginal ML via Laplace/AGQ.
use crate::{ErrorKind, StatsError, fit_logistic_with_data};
use bobyqa::{Bobyqa, Config, Status};
use nalgebra::{DMatrix, DVector, SymmetricEigen};
use statrs::distribution::{ContinuousCDF, Normal};
use std::collections::HashMap;

// Direct binomial likelihood arithmetic is verified up to this trial count;
// larger counts need a different stable log-mass calculation.
pub const MAX_LOGISTIC_MIXED_TRIALS: f64 = 1_000_000.0;

#[derive(Debug, Clone)]
pub struct LogisticMixedOptions {
    pub weights: Vec<f64>,
    pub offset: Vec<f64>,
    /// 1 = Laplace; 2..=61 = adaptive Gauss-Hermite quadrature per group.
    pub quadrature_points: usize,
    pub max_evaluations: usize,
    pub tolerance: f64,
}
impl Default for LogisticMixedOptions {
    fn default() -> Self {
        Self {
            weights: vec![],
            offset: vec![],
            quadrature_points: 1,
            max_evaluations: 20000,
            tolerance: 1e-7,
        }
    }
}
#[derive(Debug, Clone)]
pub struct LogisticMixedResult {
    pub coefficients: Vec<f64>,
    pub covariance: Vec<Vec<f64>>,
    pub std_errors: Vec<f64>,
    pub z_statistics: Vec<f64>,
    pub p_values: Vec<f64>,
    pub conf_int: Vec<[f64; 2]>,
    /// Probability at b=0, not a probability averaged over the population of groups.
    pub fixed_fitted: Vec<f64>,
    pub fitted: Vec<f64>,
    pub linear_predictor: Vec<f64>,
    pub residuals: Vec<f64>,
    /// Conditional modes, not posterior means, on the log-odds scale.
    pub random_effects: Vec<f64>,
    /// Conditional variance of each mode: the inverse conditional-mode Hessian
    /// tau² / (1 + tau² sum w mu(1-mu)) at the final iterate, as lme4
    /// ranef(condVar = TRUE) for glmer. Zero at the tau = 0 boundary.
    pub random_effects_cond_var: Vec<f64>,
    pub group_ids: Vec<usize>,
    pub group_sizes: Vec<usize>,
    pub random_intercept_variance: f64,
    pub random_intercept_sd: f64,
    pub log_likelihood: f64,
    pub neg2_log_likelihood: f64,
    pub aic: f64,
    pub bic: f64,
    pub n_parameters: usize,
    pub df_residual: i64,
    pub quadrature_points: usize,
    pub evaluations: usize,
    pub gradient_max: f64,
    pub boundary: bool,
    pub singular: bool,
}
fn err(kind: ErrorKind, s: &str) -> StatsError {
    StatsError::new(kind, s)
}
fn sigmoid(v: f64) -> f64 {
    if v >= 0.0 {
        1.0 / (1.0 + (-v).exp())
    } else {
        let e = v.exp();
        e / (1.0 + e)
    }
}
fn softplus(v: f64) -> f64 {
    v.max(0.0) + (-v.abs()).exp().ln_1p()
}

/// Standard-normal quadrature via the Hermite Jacobi matrix (Golub-Welsch).
/// Nodes and weights integrate E[f(Z)], Z~N(0,1), not exp(-z²) dz.
fn normal_quadrature(points: usize) -> Vec<(f64, f64)> {
    let mut jacobi = DMatrix::zeros(points, points);
    for j in 1..points {
        jacobi[(j - 1, j)] = (j as f64).sqrt();
        jacobi[(j, j - 1)] = (j as f64).sqrt();
    }
    let eig = SymmetricEigen::new(jacobi);
    let mut result: Vec<_> = (0..points)
        .map(|j| {
            let z = eig.eigenvalues[j];
            // Christoffel weights: 1/sum_{k=0}^{N-1} p_k(z)^2, where the
            // probabilists' Hermite polynomials are orthonormal for N(0,1).
            // Tiny eigenvector components lose relative precision at 61 nodes;
            // the polynomial sum preserves tail weights used by adaptive AGQ.
            let mut previous = 0.0;
            let mut current = 1.0;
            let mut sum = 1.0;
            for k in 1..points {
                let next = (z * current - ((k - 1) as f64).sqrt() * previous) / (k as f64).sqrt();
                sum += next * next;
                previous = current;
                current = next;
            }
            (z, 1.0 / sum)
        })
        .collect();
    result.sort_by(|a, b| a.0.total_cmp(&b.0));
    result
}
struct Data {
    x: DMatrix<f64>,
    y: Vec<f64>,
    weights: Vec<f64>,
    offset: Vec<f64>,
    members: Vec<Vec<usize>>,
    quadrature: Vec<(f64, f64)>,
    constant: f64,
}
impl Data {
    fn eta(&self, parameters: &[f64]) -> DVector<f64> {
        DVector::from_fn(self.y.len(), |i, _| {
            self.offset[i]
                + (0..self.x.ncols())
                    .map(|j| self.x[(i, j)] * parameters[j])
                    .sum::<f64>()
        })
    }
    fn group_kernel(&self, rows: &[usize], eta: &DVector<f64>, tau: f64, u: f64) -> f64 {
        // Gaussian prior is spherical: b=tau*u, u~N(0,1). Normalization
        // cancels the quadrature's standard-normal reference density below.
        -0.5 * u * u
            - rows
                .iter()
                .map(|i| {
                    let e = eta[*i] + tau * u;
                    self.weights[*i]
                        * (self.y[*i] * softplus(-e) + (1.0 - self.y[*i]) * softplus(e))
                })
                .sum::<f64>()
    }
    fn score_precision(&self, rows: &[usize], eta: &DVector<f64>, tau: f64, u: f64) -> (f64, f64) {
        let mut score = -u;
        let mut precision = 1.0;
        for i in rows {
            let e = eta[*i] + tau * u;
            score += tau
                * self.weights[*i]
                * (self.y[*i] * sigmoid(-e) - (1.0 - self.y[*i]) * sigmoid(e));
            let tail = (-e.abs()).exp();
            precision += tau * tau * self.weights[*i] * tail / (1.0 + tail).powi(2);
        }
        (score, precision)
    }
    fn mode(&self, rows: &[usize], eta: &DVector<f64>, tau: f64) -> Result<(f64, f64), StatsError> {
        if tau == 0.0 {
            return Ok((0.0, 1.0));
        }
        // The log posterior is strictly concave because the Gaussian prior
        // contributes precision 1. Safeguard Newton with a score bracket.
        let bound = tau.abs() * rows.iter().map(|i| self.weights[*i]).sum::<f64>() + 1.0;
        let mut lower = -bound;
        let mut upper = bound;
        let mut u = 0.0;
        for _ in 0..100 {
            let (score, precision) = self.score_precision(rows, eta, tau, u);
            if !score.is_finite() || !precision.is_finite() {
                return Err(err(
                    ErrorKind::Nonfinite,
                    "binomial mixed inner calculation overflowed",
                ));
            }
            if score.abs() <= 1e-11 * (1.0 + u.abs())
                || score.abs() / precision <= 1e-12 * (1.0 + u.abs())
            {
                return Ok((u, precision));
            }
            if score > 0.0 {
                lower = u;
            } else {
                upper = u;
            }
            let trial = u + score / precision;
            u = if trial > lower && trial < upper {
                trial
            } else {
                0.5 * (lower + upper)
            };
        }
        Err(err(
            ErrorKind::Nonconvergence,
            "binomial mixed conditional mode did not converge",
        ))
    }
    fn log_likelihood(&self, parameters: &[f64]) -> Result<f64, StatsError> {
        let tau = parameters[self.x.ncols()];
        let eta = self.eta(parameters);
        if eta.iter().any(|e| !e.is_finite()) {
            return Err(err(
                ErrorKind::Nonfinite,
                "linear predictor exceeds floating-point range",
            ));
        }
        let mut ll = self.constant;
        for rows in &self.members {
            let (mode, precision) = self.mode(rows, &eta, tau)?;
            let logs: Vec<f64> = self
                .quadrature
                .iter()
                .map(|(z, w)| {
                    w.ln()
                        + self.group_kernel(rows, &eta, tau, mode + z / precision.sqrt())
                        + 0.5 * z * z
                })
                .collect();
            let maximum = logs.iter().copied().fold(f64::NEG_INFINITY, f64::max);
            // Laplace is exactly the one-node case: kernel(mode)-log(h)/2.
            ll += maximum + logs.iter().map(|v| (v - maximum).exp()).sum::<f64>().ln()
                - 0.5 * precision.ln();
        }
        if !ll.is_finite() {
            return Err(err(
                ErrorKind::InvalidInput,
                "binomial mixed likelihood is not finite",
            ));
        }
        Ok(ll)
    }
}

// Joint observed information of the approximate marginal likelihood. Taking the
// fixed block after solving includes coupling to the estimated random scale.
fn information(data: &Data, parameters: &[f64], step: f64) -> Result<DMatrix<f64>, StatsError> {
    let m = parameters.len();
    let f0 = -data.log_likelihood(parameters)?;
    let steps: Vec<f64> = parameters.iter().map(|v| step * (1.0 + v.abs())).collect();
    let mut h = DMatrix::zeros(m, m);
    for i in 0..m {
        let mut a = parameters.to_vec();
        let mut b = a.clone();
        a[i] += steps[i];
        b[i] -= steps[i];
        h[(i, i)] =
            (-data.log_likelihood(&a)? - data.log_likelihood(&b)? - 2.0 * f0) / steps[i].powi(2);
        for j in 0..i {
            let mut pp = parameters.to_vec();
            let mut pm = pp.clone();
            let mut mp = pp.clone();
            let mut mm = pp.clone();
            pp[i] += steps[i];
            pp[j] += steps[j];
            pm[i] += steps[i];
            pm[j] -= steps[j];
            mp[i] -= steps[i];
            mp[j] += steps[j];
            mm[i] -= steps[i];
            mm[j] -= steps[j];
            let value = (-data.log_likelihood(&pp)?
                + data.log_likelihood(&pm)?
                + data.log_likelihood(&mp)?
                - data.log_likelihood(&mm)?)
                / (4.0 * steps[i] * steps[j]);
            h[(i, j)] = value;
            h[(j, i)] = value;
        }
    }
    Ok(h)
}

pub fn fit_logistic_random_intercept(
    x: Vec<Vec<f64>>,
    y: Vec<f64>,
    groups: Vec<usize>,
    options: LogisticMixedOptions,
) -> Result<LogisticMixedResult, StatsError> {
    let n = x.len();
    if n == 0 || x[0].is_empty() {
        return Err(err(ErrorKind::InvalidInput, "x must have rows and columns"));
    }
    let p = x[0].len();
    if n <= p || y.len() != n || groups.len() != n || x.iter().any(|r| r.len() != p) {
        return Err(err(
            ErrorKind::DimensionMismatch,
            "require rectangular finite x, proportions y in [0,1], aligned groups and n > p",
        ));
    }
    if x.iter().flatten().chain(y.iter()).any(|v| !v.is_finite()) {
        return Err(err(ErrorKind::Nonfinite, "x and y must be finite"));
    }
    if y.iter().any(|v| !(0.0..=1.0).contains(v)) {
        return Err(err(
            ErrorKind::InvalidInput,
            "binomial y must be proportions in [0,1]",
        ));
    }
    if !(1..=61).contains(&options.quadrature_points) {
        return Err(err(
            ErrorKind::InvalidInput,
            "quadraturePoints must be 1 (Laplace) or 2..61 (adaptive quadrature)",
        ));
    }
    if !options.tolerance.is_finite() || options.tolerance <= 0.0 || options.tolerance > 0.1 {
        return Err(err(
            ErrorKind::Nonfinite,
            "tolerance must be finite in (0,0.1]",
        ));
    }
    let (weights, offset) = crate::observations::prepare(n, options.weights, options.offset, true)?;
    if weights.iter().any(|w| *w > MAX_LOGISTIC_MIXED_TRIALS)
        || y.iter()
            .zip(&weights)
            .any(|(y, w)| (y * w - (y * w).round()).abs() > 1e-8)
    {
        return Err(err(
            ErrorKind::InvalidInput,
            "require integer trials <= 1000000 and integer success counts",
        ));
    }
    // Canonicalize floating-point encodings of integer successes. Without this,
    // tiny near-zero proportions could create fictitious successes and a finite
    // fit for what is physically an all-failure sample.
    let y: Vec<f64> = y
        .iter()
        .zip(&weights)
        .map(|(y, w)| (y * w).round() / w)
        .collect();
    if y.iter().all(|v| *v == 0.0) || y.iter().all(|v| *v == 1.0) {
        return Err(err(
            ErrorKind::InvalidInput,
            "require successes and failures for a finite unpenalized fit",
        ));
    }
    let mut group_ids = Vec::new();
    let mut members: Vec<Vec<usize>> = Vec::new();
    let mut map = HashMap::new();
    let indices: Vec<usize> = groups
        .into_iter()
        .enumerate()
        .map(|(i, id)| {
            let g = *map.entry(id).or_insert_with(|| {
                group_ids.push(id);
                members.push(Vec::new());
                members.len() - 1
            });
            members[g].push(i);
            g
        })
        .collect();
    if members.len() < 2 || members.len() == n {
        return Err(err(
            ErrorKind::InvalidInput,
            "require at least two groups and repeated observations",
        ));
    }
    let design = DMatrix::from_row_iterator(n, p, x.iter().flatten().copied());
    // RMS scaling retains manageable optimizer coefficient units, including ones.
    let scales: Vec<f64> = (0..p)
        .map(|j| design.column(j).norm() / (n as f64).sqrt())
        .collect();
    if scales.iter().any(|s| !s.is_finite() || *s == 0.0) {
        return Err(err(
            ErrorKind::RankDeficiency,
            "fixed design is rank deficient or has invalid scale",
        ));
    }
    let scaled = DMatrix::from_fn(n, p, |i, j| design[(i, j)] / scales[j]);
    let svd = scaled.clone().svd(false, false);
    let rank_tolerance = f64::EPSILON * n.max(p) as f64 * svd.singular_values[0];
    if svd.singular_values.iter().any(|s| *s <= rank_tolerance) {
        return Err(err(
            ErrorKind::RankDeficiency,
            "fixed design is numerically rank deficient",
        ));
    }
    let constant = y
        .iter()
        .zip(&weights)
        .map(|(v, w)| {
            if *v == 0.0 || *v == 1.0 {
                0.0
            } else {
                let k = v * w;
                statrs::function::gamma::ln_gamma(w + 1.0)
                    - statrs::function::gamma::ln_gamma(k + 1.0)
                    - statrs::function::gamma::ln_gamma(w - k + 1.0)
            }
        })
        .sum::<f64>();
    if !constant.is_finite() {
        return Err(err(
            ErrorKind::Nonfinite,
            "binomial likelihood constant exceeds floating-point range",
        ));
    }
    // Separation in the fixed design also prevents a finite unpenalized GLMM MLE.
    let glm = fit_logistic_with_data(x, y.clone(), weights.clone(), offset.clone())?;
    let data = Data {
        x: scaled,
        y: y.clone(),
        weights,
        offset,
        members,
        quadrature: normal_quadrature(options.quadrature_points),
        constant,
    };
    let mut parameters: Vec<f64> = (0..p).map(|j| glm.coefficients[j] * scales[j]).collect();
    parameters.push(1.0);
    let mut lower = vec![-100.0; p + 1];
    lower[p] = 0.0;
    let mut upper = vec![100.0; p + 1];
    upper[p] = 20.0;
    if parameters
        .iter()
        .zip(&lower)
        .zip(&upper)
        .any(|((v, l), u)| *v <= *l || *v >= *u)
    {
        return Err(err(
            ErrorKind::InvalidInput,
            "initial fixed effects exceed mixed-model optimizer bounds; rescale/center predictors",
        ));
    }
    let mut config = Config::new(p + 1);
    config.rho_begin = 0.1;
    config.rho_end = options.tolerance;
    config.max_fun = options.max_evaluations;
    let mut optimizer = Bobyqa::new(p + 1, config).map_err(|e| {
        err(
            ErrorKind::InvalidInput,
            &format!("invalid binomial mixed control: {e}"),
        )
    })?;
    let outcome = optimizer.minimize(
        |t| data.log_likelihood(t).map_or(f64::INFINITY, |ll| -ll),
        &mut parameters,
        &lower,
        &upper,
    );
    if outcome.status != Status::Converged || !outcome.found_finite() {
        return Err(err(
            ErrorKind::InvalidInput,
            &format!("binomial mixed optimizer failed: {}", outcome.status),
        ));
    }
    if parameters.iter().take(p).any(|v| v.abs() >= 99.9) || parameters[p] >= 19.9 {
        return Err(err(
            ErrorKind::Nonconvergence,
            "binomial mixed fit reaches search bound; separation/weak identification may be present",
        ));
    }
    let mut likelihood = data.log_likelihood(&parameters)?;
    if glm.log_likelihood >= likelihood
        || (parameters[p] < 1e-4
            && likelihood - glm.log_likelihood <= 64.0 * f64::EPSILON * (1.0 + likelihood.abs()))
    {
        for j in 0..p {
            parameters[j] = glm.coefficients[j] * scales[j];
        }
        parameters[p] = 0.0;
        likelihood = glm.log_likelihood;
    }
    let boundary = parameters[p] == 0.0;
    let tau = parameters[p];
    let covariance = if boundary {
        DMatrix::from_fn(p, p, |i, j| glm.covariance[i][j])
    } else {
        let coarse = information(&data, &parameters, 1e-3)?;
        let fine = information(&data, &parameters, 5e-4)?;
        let hessian = (4.0 * fine - coarse) / 3.0;
        let cholesky = hessian.cholesky().ok_or_else(|| {
            err(
                ErrorKind::FactorizationFailure,
                "marginal observed information is not positive definite; inference is unreliable",
            )
        })?;
        let inverse = cholesky.solve(&DMatrix::identity(p + 1, p + 1));
        DMatrix::from_fn(p, p, |i, j| inverse[(i, j)] / scales[i] / scales[j])
    };
    let beta: Vec<f64> = (0..p).map(|j| parameters[j] / scales[j]).collect();
    let eta = data.eta(&parameters);
    let (effects, cond_var): (Vec<f64>, Vec<f64>) = data
        .members
        .iter()
        .map(|rows| {
            data.mode(rows, &eta, tau)
                .map(|(u, precision)| (tau * u, tau * tau / precision))
        })
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .unzip();
    let linear_predictor: Vec<f64> = (0..n).map(|i| eta[i] + effects[indices[i]]).collect();
    let fitted: Vec<f64> = linear_predictor.iter().map(|e| sigmoid(*e)).collect();
    let std_errors: Vec<f64> = (0..p).map(|j| covariance[(j, j)].sqrt()).collect();
    let z_statistics: Vec<f64> = (0..p).map(|j| beta[j] / std_errors[j]).collect();
    let normal = Normal::new(0.0, 1.0)
        .map_err(|_| err(ErrorKind::Internal, "invalid normal distribution"))?;
    let critical = normal.inverse_cdf(0.975);
    let mut gradient_max = 0.0_f64;
    for j in 0..p + 1 {
        if boundary && j == p {
            continue;
        }
        let h = 1e-5 * (1.0 + parameters[j].abs());
        let mut a = parameters.clone();
        let mut b = a.clone();
        a[j] += h;
        b[j] -= h;
        gradient_max = gradient_max
            .max(((data.log_likelihood(&a)? - data.log_likelihood(&b)?) / (2.0 * h)).abs());
    }
    if !gradient_max.is_finite() || gradient_max > 1e-3 {
        return Err(err(
            ErrorKind::Nonconvergence,
            "binomial mixed optimizer stopped with a nonstationary likelihood score",
        ));
    }
    let n_parameters = p + 1;
    let result = LogisticMixedResult {
        coefficients: beta.clone(),
        covariance: (0..p)
            .map(|i| (0..p).map(|j| covariance[(i, j)]).collect())
            .collect(),
        std_errors: std_errors.clone(),
        p_values: z_statistics
            .iter()
            .map(|z| 2.0 * normal.sf(z.abs()))
            .collect(),
        conf_int: (0..p)
            .map(|j| {
                [
                    beta[j] - critical * std_errors[j],
                    beta[j] + critical * std_errors[j],
                ]
            })
            .collect(),
        z_statistics,
        fixed_fitted: eta.iter().map(|e| sigmoid(*e)).collect(),
        residuals: y.iter().zip(&fitted).map(|(y, p)| y - p).collect(),
        fitted,
        linear_predictor,
        random_effects: effects,
        random_effects_cond_var: cond_var,
        group_ids,
        group_sizes: data.members.iter().map(Vec::len).collect(),
        random_intercept_variance: tau * tau,
        random_intercept_sd: tau,
        log_likelihood: likelihood,
        neg2_log_likelihood: -2.0 * likelihood,
        aic: 2.0 * n_parameters as f64 - 2.0 * likelihood,
        bic: n_parameters as f64 * (n as f64).ln() - 2.0 * likelihood,
        n_parameters,
        df_residual: n as i64 - n_parameters as i64,
        quadrature_points: options.quadrature_points,
        evaluations: outcome.n_eval,
        gradient_max,
        boundary,
        singular: tau < 1e-4,
    };
    if result
        .coefficients
        .iter()
        .chain(result.covariance.iter().flatten())
        .chain(result.std_errors.iter())
        .chain(result.z_statistics.iter())
        .chain(result.fitted.iter())
        .chain(result.random_effects.iter())
        .chain(result.random_effects_cond_var.iter())
        .any(|v| !v.is_finite())
    {
        return Err(err(
            ErrorKind::Nonfinite,
            "binomial mixed results exceed floating-point range",
        ));
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn normal_quadrature_integrates_normal_moments() {
        for points in [1, 3, 9, 25, 61] {
            let q = normal_quadrature(points);
            assert!((q.iter().map(|(_, w)| w).sum::<f64>() - 1.0).abs() < 1e-13);
            assert!(q.iter().map(|(z, w)| z * w).sum::<f64>().abs() < 1e-13);
            if points >= 3 {
                assert!((q.iter().map(|(z, w)| z * z * w).sum::<f64>() - 1.0).abs() < 1e-13);
            }
            if points >= 9 {
                assert!((q.iter().map(|(z, w)| z.powi(4) * w).sum::<f64>() - 3.0).abs() < 1e-12);
            }
        }
    }
    #[test]
    fn tiny_quadrature_tail_weights_keep_relative_precision() {
        // Independent NumPy hermegauss(N), normalized by sqrt(2*pi).
        for (points, node, weight) in [
            (9, -4.512745863399783, 2.23458440077466e-5),
            (25, -8.717597678399587, 1.530038997998717e-17),
            (61, -14.498533915900149, 9.371228767883732e-47),
        ] {
            let q = normal_quadrature(points);
            assert!((q[0].0 - node).abs() < 1e-12);
            assert!((q[0].1 / weight - 1.0).abs() < 1e-10);
        }
    }
}
