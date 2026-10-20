//! Gaussian mixed models using square-root penalized least squares.
//! Explicit random-design terms; no formula parser, no observation covariance inverse.
use crate::{ErrorKind, StatsError};
use argmin::{
    core::{CostFunction, Executor, State, TerminationReason},
    solver::neldermead::NelderMead,
};
use bobyqa::{Bobyqa, Config, Status};
use nalgebra::{DMatrix, DVector};
use statrs::distribution::{ContinuousCDF, Normal};
use std::{cell::Cell, collections::HashMap};

#[derive(Debug, Clone)]
pub struct RandomTerm {
    /// n rows, k columns: supply random intercept ones and/or observed slope values.
    pub z: Vec<Vec<f64>>,
    pub groups: Vec<usize>,
    /// false constrains the within-term covariance to diagonal.
    pub correlated: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LmmOptimizer {
    Bobyqa,
    NelderMead,
}
impl LmmOptimizer {
    pub fn name(self) -> &'static str {
        match self {
            Self::Bobyqa => "bobyqa",
            Self::NelderMead => "nelder_mead",
        }
    }
}
#[derive(Debug, Clone)]
pub struct LmmOptions {
    pub reml: bool,
    pub optimizer: LmmOptimizer,
    pub objective_tolerance: f64,
    pub weights: Vec<f64>,
    pub offset: Vec<f64>,
    /// Lower-triangular factors, column-major within each term, then term order.
    pub start: Option<Vec<f64>>,
    pub max_evaluations: usize,
    pub tolerance: f64,
}
impl Default for LmmOptions {
    fn default() -> Self {
        Self {
            reml: false,
            optimizer: LmmOptimizer::Bobyqa,
            objective_tolerance: 1e-12,
            weights: vec![],
            offset: vec![],
            start: None,
            max_evaluations: 20000,
            tolerance: 1e-7,
        }
    }
}
#[derive(Debug, Clone)]
pub struct RandomTermResult {
    pub covariance: Vec<Vec<f64>>,
    pub effects: Vec<Vec<f64>>,
    /// Per group, the k x k conditional covariance of that group's random-effect
    /// vector given y at the estimates: the matching diagonal block of
    /// sigma² Λ (Λ'Z'WZΛ + I)⁻¹ Λ', as lme4 ranef(condVar = TRUE).
    pub effects_cond_var: Vec<Vec<Vec<f64>>>,
    pub group_ids: Vec<usize>,
    pub group_sizes: Vec<usize>,
}
#[derive(Debug, Clone)]
pub struct LmmResult {
    pub coefficients: Vec<f64>,
    pub covariance: Vec<Vec<f64>>,
    pub std_errors: Vec<f64>,
    pub z_statistics: Vec<f64>,
    pub p_values: Vec<f64>,
    pub conf_int: Vec<[f64; 2]>,
    pub fixed_fitted: Vec<f64>,
    pub fitted: Vec<f64>,
    pub residuals: Vec<f64>,
    pub marginal_residuals: Vec<f64>,
    pub terms: Vec<RandomTermResult>,
    pub theta: Vec<f64>,
    pub residual_variance: f64,
    pub sigma: f64,
    /// REML restricted likelihood when reml=true, marginal likelihood otherwise.
    pub log_likelihood: f64,
    pub aic: f64,
    pub bic: f64,
    /// Matches lme4 df.residual: n minus all likelihood parameters.
    pub df_residual: i64,
    pub n_parameters: usize,
    pub evaluations: usize,
    pub optimizer: LmmOptimizer,
    pub singular: bool,
    pub reml: bool,
}
fn err(kind: ErrorKind, s: &str) -> StatsError {
    StatsError::new(kind, s)
}
fn rows(m: &DMatrix<f64>) -> Vec<Vec<f64>> {
    (0..m.nrows())
        .map(|i| (0..m.ncols()).map(|j| m[(i, j)]).collect())
        .collect()
}
struct TermData {
    k: usize,
    base: usize,
    ids: Vec<usize>,
    sizes: Vec<usize>,
    indices: Vec<usize>,
    // (row, column, global theta index), in lme4's lower-triangle order.
    parameters: Vec<(usize, usize, usize)>,
}
struct Data {
    r: DMatrix<f64>, // QR summary of weighted [Z, X_scaled, y-offset]
    terms: Vec<TermData>,
    n: usize,
    p: usize,
    q: usize,
    scales: Vec<f64>,
    log_weights: f64,
    reml: bool,
}
impl Data {
    fn lambda(&self, theta: &[f64]) -> DMatrix<f64> {
        let mut l = DMatrix::zeros(self.q, self.q);
        for term in &self.terms {
            for g in 0..term.ids.len() {
                for &(i, j, t) in &term.parameters {
                    l[(term.base + g * term.k + i, term.base + g * term.k + j)] = theta[t];
                }
            }
        }
        l
    }
    fn profile(&self, theta: &[f64]) -> Result<Profile, StatsError> {
        let lambda = self.lambda(theta);
        let u = self.r.columns(0, self.q) * &lambda;
        let nr = self.r.nrows();
        // [R_Z Lambda  R_X R_y; I 0 0]. Its residual norm is the
        // weighted observation RSS plus ||u_random||², exactly the PLS criterion.
        let mut a = DMatrix::zeros(nr + self.q, self.q + self.p);
        a.view_mut((0, 0), (nr, self.q)).copy_from(&u);
        a.view_mut((0, self.q), (nr, self.p))
            .copy_from(&self.r.columns(self.q, self.p));
        for j in 0..self.q {
            a[(nr + j, j)] = 1.0;
        }
        let b = DVector::from_fn(nr + self.q, |i, _| {
            if i < nr {
                self.r[(i, self.q + self.p)]
            } else {
                0.0
            }
        });
        let qr = a.clone().qr();
        let r = qr.r();
        let mut qt_b = b.clone();
        qr.q_tr_mul(&mut qt_b);
        let upper = r.rows(0, self.q + self.p).into_owned();
        let solution = upper
            .solve_upper_triangular(&qt_b.rows(0, self.q + self.p).into_owned())
            .ok_or_else(|| {
                err(
                    ErrorKind::RankDeficiency,
                    "mixed-model fixed design is rank deficient",
                )
            })?;
        // QR compression retains residual directions, including components of y
        // perpendicular to both Z and X. Do not use y'y - beta'X'y subtraction.
        let q = (b - a * &solution).norm_squared();
        let df = self.n - if self.reml { self.p } else { 0 };
        let variance = q / df as f64;
        if !variance.is_finite() || variance <= 0.0 {
            return Err(err(
                ErrorKind::InvalidInput,
                "no positive finite residual variance",
            ));
        }
        let fixed_r = upper.view((self.q, self.q), (self.p, self.p)).into_owned();
        let svd = fixed_r.clone().svd(false, false);
        let cutoff = f64::EPSILON * self.n.max(self.p) as f64 * svd.singular_values[0];
        if svd
            .singular_values
            .iter()
            .any(|v| !v.is_finite() || *v <= cutoff)
        {
            return Err(err(
                ErrorKind::RankDeficiency,
                "mixed-model fixed design/information is numerically rank deficient",
            ));
        }
        // The random block of R satisfies R_U'R_U=I+U'U.
        let logdet_v =
            2.0 * (0..self.q).map(|j| upper[(j, j)].abs().ln()).sum::<f64>() - self.log_weights;
        let logdet_x = if self.reml {
            2.0 * ((0..self.p).map(|j| fixed_r[(j, j)].abs().ln()).sum::<f64>()
                + self.scales.iter().map(|s| s.ln()).sum::<f64>())
        } else {
            0.0
        };
        let objective = 0.5
            * (df as f64 * ((2.0 * std::f64::consts::PI).ln() + variance.ln() + 1.0)
                + logdet_v
                + logdet_x);
        if !objective.is_finite() {
            return Err(err(
                ErrorKind::Nonfinite,
                "mixed likelihood exceeds floating-point range",
            ));
        }
        Ok(Profile {
            solution,
            random_r: upper.view((0, 0), (self.q, self.q)).into_owned(),
            fixed_r,
            variance,
            objective,
            lambda,
        })
    }
}
// Internal adapter only: the statistical API remains specific to Gaussian fits.
struct SimplexObjective<'a> {
    data: &'a Data,
    lower: &'a [f64],
    evaluations: &'a Cell<usize>,
    budget: usize,
}
impl CostFunction for SimplexObjective<'_> {
    type Param = Vec<f64>;
    type Output = f64;
    fn cost(&self, theta: &Vec<f64>) -> Result<f64, argmin::core::Error> {
        if self.evaluations.get() >= self.budget {
            return Err(argmin::core::Error::msg(
                "Nelder-Mead evaluation budget exhausted",
            ));
        }
        self.evaluations.set(self.evaluations.get() + 1);
        if theta
            .iter()
            .zip(self.lower)
            .any(|(v, l)| !v.is_finite() || *v < *l || *v > 1e4)
        {
            return Ok(f64::INFINITY);
        }
        Ok(self
            .data
            .profile(theta)
            .map_or(f64::INFINITY, |fit| fit.objective))
    }
}

struct Profile {
    solution: DVector<f64>,
    // R_11 with R_11'R_11 = Λ'Z'WZΛ + I (the penalized random-effect block).
    random_r: DMatrix<f64>,
    fixed_r: DMatrix<f64>,
    variance: f64,
    objective: f64,
    lambda: DMatrix<f64>,
}

/// Fit Gaussian random intercepts/slopes, crossed/nested explicit grouping terms,
/// weights Var(e_i)=sigma²/weight_i and additive offsets. ML by default.
pub fn fit_lmm(
    x: Vec<Vec<f64>>,
    y: Vec<f64>,
    terms: Vec<RandomTerm>,
    options: LmmOptions,
) -> Result<LmmResult, StatsError> {
    let n = x.len();
    if n == 0 || x[0].is_empty() {
        return Err(err(ErrorKind::InvalidInput, "x must have rows and columns"));
    }
    let p = x[0].len();
    if n <= p || y.len() != n || x.iter().any(|r| r.len() != p) {
        return Err(err(
            ErrorKind::DimensionMismatch,
            "require rectangular finite x, finite y, and n > p",
        ));
    }
    if x.iter().flatten().chain(y.iter()).any(|v| !v.is_finite()) {
        return Err(err(ErrorKind::Nonfinite, "x and y must be finite"));
    }
    if terms.is_empty() {
        return Err(err(
            ErrorKind::InvalidInput,
            "supply at least one random term",
        ));
    }
    let weights = if options.weights.is_empty() {
        vec![1.0; n]
    } else {
        options.weights
    };
    let offset = if options.offset.is_empty() {
        vec![0.0; n]
    } else {
        options.offset
    };
    if weights.len() != n
        || offset.len() != n
        || weights.iter().any(|v| !v.is_finite() || *v <= 0.0)
        || offset.iter().any(|v| !v.is_finite())
    {
        return Err(err(
            ErrorKind::Nonfinite,
            "weights must be positive finite and offset finite, one value per row",
        ));
    }
    if !options.tolerance.is_finite() || options.tolerance <= 0.0 || options.tolerance > 0.1 {
        return Err(err(
            ErrorKind::Nonfinite,
            "tolerance must be finite in (0, 0.1]",
        ));
    }
    let mut metadata = Vec::new();
    let mut q = 0usize;
    let mut nt = 0usize;
    let mut lower = Vec::new();
    let mut initial = Vec::new();
    for term in &terms {
        if term.z.len() != n || term.groups.len() != n || term.z[0].is_empty() {
            return Err(err(
                ErrorKind::InvalidInput,
                "each random term needs n nonempty z rows and n groups",
            ));
        }
        let k = term.z[0].len();
        if term.z.iter().any(|r| r.len() != k) || term.z.iter().flatten().any(|v| !v.is_finite()) {
            return Err(err(
                ErrorKind::DimensionMismatch,
                "random z must be rectangular and finite",
            ));
        }
        let mut ids = Vec::new();
        let mut sizes = Vec::new();
        let mut map = HashMap::new();
        let indices: Vec<usize> = term
            .groups
            .iter()
            .map(|id| {
                let g = *map.entry(*id).or_insert_with(|| {
                    ids.push(*id);
                    sizes.push(0);
                    ids.len() - 1
                });
                sizes[g] += 1;
                g
            })
            .collect();
        if ids.len() < 2 || ids.len() == n {
            return Err(err(
                ErrorKind::InvalidInput,
                "each term needs >=2 groups and repeated observations",
            ));
        }
        if term.correlated && k.checked_mul(ids.len()).is_none_or(|q| q >= n) {
            return Err(err(
                ErrorKind::Unsupported,
                "correlated random term requires more observations than random coefficients (lme4 default identifiability check)",
            ));
        }
        let mut parameters = Vec::new();
        for j in 0..k {
            for i in j..k {
                if i == j || term.correlated {
                    parameters.push((i, j, nt));
                    nt += 1;
                    lower.push(if i == j { 0.0 } else { -1e4 });
                    initial.push(if i == j { 1.0 } else { 0.0 });
                }
            }
        }
        metadata.push(TermData {
            k,
            base: q,
            ids,
            sizes,
            indices,
            parameters,
        });
        q = q
            .checked_add(
                k.checked_mul(map.len())
                    .ok_or_else(|| err(ErrorKind::Unsupported, "random design too large"))?,
            )
            .ok_or_else(|| err(ErrorKind::Unsupported, "random design too large"))?;
    }
    // This first general implementation uses a dense square-root summary, not
    // lme4's sparse Cholesky. Fail explicitly rather than allocate unbounded memory.
    if q > 512 || n.checked_mul(q + p + 1).is_none_or(|s| s > 50_000_000) {
        return Err(err(
            ErrorKind::Unsupported,
            "general mixed-model dense design limit exceeded (q <= 512, n*(q+p+1) <= 50000000); use fit_mixed_intercept for large random-intercept fits",
        ));
    }
    let design = DMatrix::from_row_iterator(n, p, x.into_iter().flatten());
    let scales: Vec<f64> = (0..p)
        .map(|j| {
            (0..n)
                .map(|i| weights[i] * design[(i, j)].powi(2))
                .sum::<f64>()
                .sqrt()
        })
        .collect();
    if scales.iter().any(|s| !s.is_finite() || *s == 0.0) {
        return Err(err(
            ErrorKind::RankDeficiency,
            "fixed design is rank deficient or has invalid scale",
        ));
    }
    let mut augmented = DMatrix::zeros(n, q + p + 1);
    for i in 0..n {
        let w = weights[i].sqrt();
        for (term, meta) in terms.iter().zip(&metadata) {
            for j in 0..meta.k {
                augmented[(i, meta.base + meta.indices[i] * meta.k + j)] = w * term.z[i][j];
            }
        }
        for j in 0..p {
            augmented[(i, q + j)] = w * design[(i, j)] / scales[j];
        }
        augmented[(i, q + p)] = w * (y[i] - offset[i]);
    }
    if augmented.iter().any(|v| !v.is_finite()) {
        return Err(err(
            ErrorKind::Nonfinite,
            "weighted design exceeds floating-point range",
        ));
    }
    let data = Data {
        r: augmented.qr().unpack_r(),
        terms: metadata,
        n,
        p,
        q,
        scales,
        log_weights: weights.iter().map(|w| w.ln()).sum(),
        reml: options.reml,
    };
    // Validate rank and positive variance before passing any objective to BOBYQA.
    let zero = vec![0.0; nt];
    let zero_fit = data.profile(&zero)?;
    let outcome_scale = y
        .iter()
        .zip(&offset)
        .zip(&weights)
        .map(|((y, o), w)| w * (y - o).powi(2))
        .sum::<f64>();
    if zero_fit.variance <= f64::EPSILON.powi(2) * (n as f64).powi(2) * outcome_scale / n as f64 {
        return Err(err(
            ErrorKind::InvalidInput,
            "numerically perfect fit: no positive residual-variance optimum",
        ));
    }
    let mut theta = options.start.unwrap_or(initial);
    if theta.len() != nt
        || theta
            .iter()
            .zip(&lower)
            .any(|(v, l)| !v.is_finite() || *v < *l || *v > 1e4)
    {
        return Err(err(
            ErrorKind::DimensionMismatch,
            "start must match theta length and bounds",
        ));
    }
    if !options.objective_tolerance.is_finite() || options.objective_tolerance <= 0.0 {
        return Err(err(
            ErrorKind::InvalidInput,
            "objectiveTolerance must be positive finite",
        ));
    }
    let evaluations = match options.optimizer {
        LmmOptimizer::Bobyqa => {
            let mut config = Config::new(nt);
            config.rho_begin = 0.1;
            config.rho_end = options.tolerance;
            config.max_fun = options.max_evaluations;
            let mut optimizer = Bobyqa::new(nt, config).map_err(|e| {
                err(
                    ErrorKind::InvalidInput,
                    &format!("invalid BOBYQA control: {e}"),
                )
            })?;
            let outcome = optimizer.minimize(
                |t| data.profile(t).map_or(f64::INFINITY, |fit| fit.objective),
                &mut theta,
                &lower,
                &vec![1e4; nt],
            );
            if outcome.status != Status::Converged || !outcome.found_finite() {
                return Err(err(
                    ErrorKind::Nonconvergence,
                    &format!("mixed-model BOBYQA failed: {}", outcome.status),
                ));
            }
            outcome.n_eval
        }
        LmmOptimizer::NelderMead => {
            let counter = Cell::new(0);
            let mut simplex = vec![theta.clone()];
            for j in 0..nt {
                let mut vertex = theta.clone();
                vertex[j] = if vertex[j] + 0.1 > 1e4 {
                    vertex[j] - 0.1
                } else {
                    vertex[j] + 0.1
                };
                simplex.push(vertex);
            }
            let solver = NelderMead::new(simplex)
                .with_sd_tolerance(options.objective_tolerance)
                .map_err(|e| {
                    err(
                        ErrorKind::InvalidInput,
                        &format!("invalid Nelder-Mead control: {e}"),
                    )
                })?;
            let result = Executor::new(
                SimplexObjective {
                    data: &data,
                    lower: &lower,
                    evaluations: &counter,
                    budget: options.max_evaluations,
                },
                solver,
            )
            .configure(|state| state.max_iters(options.max_evaluations as u64))
            .run()
            .map_err(|e| {
                err(
                    ErrorKind::Nonconvergence,
                    &format!("mixed-model Nelder-Mead failed: {e}"),
                )
            })?;
            if result.state.get_termination_reason() != Some(&TerminationReason::SolverConverged)
                || !result.state.get_best_cost().is_finite()
            {
                return Err(err(
                    ErrorKind::Nonconvergence,
                    "mixed-model Nelder-Mead did not converge",
                ));
            }
            theta = result
                .state
                .get_best_param()
                .ok_or_else(|| err(ErrorKind::Internal, "Nelder-Mead returned no parameters"))?
                .clone();
            counter.get()
        }
    };
    if theta.iter().any(|v| v.abs() >= 9999.0) {
        return Err(err(
            ErrorKind::Nonconvergence,
            "optimum reaches theta search bound",
        ));
    }
    // The all-zero covariance is an admissible boundary, not a log-variance limit.
    let mut fit = data.profile(&theta)?;
    if zero_fit.objective < fit.objective {
        theta = zero;
        fit = zero_fit;
    }
    let beta = DVector::from_fn(p, |j, _| fit.solution[q + j] / data.scales[j]);
    let r_inverse = fit
        .fixed_r
        .solve_upper_triangular(&DMatrix::identity(p, p))
        .ok_or_else(|| {
            err(
                ErrorKind::FactorizationFailure,
                "cannot solve fixed-effect covariance factor",
            )
        })?;
    let scaled_cov = &r_inverse * r_inverse.transpose();
    let covariance = DMatrix::from_fn(p, p, |i, j| {
        fit.variance * scaled_cov[(i, j)] / data.scales[i] / data.scales[j]
    });
    let std_errors: Vec<f64> = (0..p).map(|j| covariance[(j, j)].sqrt()).collect();
    let z_statistics: Vec<f64> = (0..p).map(|j| beta[j] / std_errors[j]).collect();
    let normal = Normal::new(0.0, 1.0)
        .map_err(|_| err(ErrorKind::Internal, "invalid normal distribution"))?;
    let critical = normal.inverse_cdf(0.975);
    let effects = &fit.lambda * fit.solution.rows(0, q);
    let fixed: Vec<f64> = (0..n)
        .map(|i| offset[i] + (0..p).map(|j| design[(i, j)] * beta[j]).sum::<f64>())
        .collect();
    let fitted: Vec<f64> = (0..n)
        .map(|i| {
            fixed[i]
                + terms
                    .iter()
                    .zip(&data.terms)
                    .map(|(t, m)| {
                        (0..m.k)
                            .map(|j| t.z[i][j] * effects[m.base + m.indices[i] * m.k + j])
                            .sum::<f64>()
                    })
                    .sum::<f64>()
        })
        .collect();
    // lme4 condVar: sigma² Λ (L L')⁻¹ Λ' with L L' = Λ'Z'WZΛ + I at the final
    // theta. With R_11'R_11 = L L', solve R_11' S = Λ' so the product is S'S.
    let spread = fit
        .random_r
        .transpose()
        .solve_lower_triangular(&fit.lambda.transpose())
        .ok_or_else(|| {
            err(
                ErrorKind::FactorizationFailure,
                "cannot solve random-effect conditional covariance factor",
            )
        })?;
    let cond_block = |base: usize, k: usize| -> Vec<Vec<f64>> {
        (0..k)
            .map(|i| {
                (0..k)
                    .map(|j| fit.variance * spread.column(base + i).dot(&spread.column(base + j)))
                    .collect()
            })
            .collect()
    };
    let mut singular = false;
    let term_results = data
        .terms
        .iter()
        .map(|m| {
            let mut l = DMatrix::zeros(m.k, m.k);
            for &(i, j, t) in &m.parameters {
                l[(i, j)] = theta[t];
            }
            // lme4 isSingular checks near-zero diagonals of relative Cholesky factors.
            singular |= (0..m.k).any(|j| l[(j, j)] < 1e-4);
            RandomTermResult {
                covariance: rows(&(fit.variance * &l * l.transpose())),
                effects: (0..m.ids.len())
                    .map(|g| (0..m.k).map(|j| effects[m.base + g * m.k + j]).collect())
                    .collect(),
                effects_cond_var: (0..m.ids.len())
                    .map(|g| cond_block(m.base + g * m.k, m.k))
                    .collect(),
                group_ids: m.ids.clone(),
                group_sizes: m.sizes.clone(),
            }
        })
        .collect();
    let n_parameters = p + nt + 1;
    let result = LmmResult {
        coefficients: beta.iter().copied().collect(),
        covariance: rows(&covariance),
        std_errors,
        p_values: z_statistics
            .iter()
            .map(|z| 2.0 * normal.sf(z.abs()))
            .collect(),
        conf_int: (0..p)
            .map(|j| {
                [
                    beta[j] - critical * covariance[(j, j)].sqrt(),
                    beta[j] + critical * covariance[(j, j)].sqrt(),
                ]
            })
            .collect(),
        z_statistics,
        residuals: y.iter().zip(&fitted).map(|(y, f)| y - f).collect(),
        marginal_residuals: y.iter().zip(&fixed).map(|(y, f)| y - f).collect(),
        fixed_fitted: fixed,
        fitted,
        terms: term_results,
        theta,
        residual_variance: fit.variance,
        sigma: fit.variance.sqrt(),
        log_likelihood: -fit.objective,
        aic: 2.0 * n_parameters as f64 + 2.0 * fit.objective,
        bic: n_parameters as f64 * (n as f64).ln() + 2.0 * fit.objective,
        df_residual: n as i64 - n_parameters as i64,
        n_parameters,
        evaluations: evaluations + 2,
        optimizer: options.optimizer,
        singular,
        reml: options.reml,
    };
    if result
        .coefficients
        .iter()
        .chain(result.covariance.iter().flatten())
        .chain(result.fitted.iter())
        .chain(
            result
                .terms
                .iter()
                .flat_map(|t| t.effects_cond_var.iter().flatten().flatten()),
        )
        .chain(result.std_errors.iter())
        .chain(result.z_statistics.iter())
        .any(|v| !v.is_finite())
    {
        return Err(err(
            ErrorKind::Nonfinite,
            "mixed-model output exceeds floating-point range",
        ));
    }
    Ok(result)
}
