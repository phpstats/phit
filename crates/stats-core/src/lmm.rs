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
/// The random-effect columns that share rows with each other (a connected component of the design), and the
/// QR summary of the weighted rows they appear in: [Z_block, X_scaled, y-offset]. One grouping term gives one
/// block per group; nested terms give one per outer group.
struct Block {
    columns: Vec<usize>,
    r: DMatrix<f64>,
}
struct Data {
    /// QR summary of weighted [Z, X_scaled, y-offset], when the design does not split into blocks.
    r: DMatrix<f64>,
    /// The design split into independent blocks, which each evaluation factors separately.
    blocks: Option<Vec<Block>>,
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
        if let Some(blocks) = &self.blocks {
            return self.profile_blocks(blocks, theta);
        }
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
        let fixed_r = upper.view((self.q, self.q), (self.p, self.p)).into_owned();
        let logdet_r = (0..self.q).map(|j| upper[(j, j)].abs().ln()).sum::<f64>();
        self.finish(
            solution,
            q,
            fixed_r,
            logdet_r,
            Random::Dense {
                random_r: upper.view((0, 0), (self.q, self.q)).into_owned(),
                lambda,
            },
        )
    }

    /// The profiled objective from the penalized least-squares solution, its residual sum of squares, the fixed
    /// block of the factor, and the log determinant of the random block's factor.
    fn finish(
        &self,
        solution: DVector<f64>,
        q: f64,
        fixed_r: DMatrix<f64>,
        logdet_r: f64,
        random: Random,
    ) -> Result<Profile, StatsError> {
        let df = self.n - if self.reml { self.p } else { 0 };
        let variance = q / df as f64;
        if !variance.is_finite() || variance <= 0.0 {
            return Err(err(
                ErrorKind::InvalidInput,
                "no positive finite residual variance",
            ));
        }
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
        let logdet_v = 2.0 * logdet_r - self.log_weights;
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
            fixed_r,
            variance,
            objective,
            random,
        })
    }

    /// One block's Λ: each term-group's k x k factor at that group's columns.
    fn block_lambda(&self, columns: &[usize], theta: &[f64]) -> DMatrix<f64> {
        let m = columns.len();
        let mut l = DMatrix::zeros(m, m);
        let local: HashMap<usize, usize> =
            columns.iter().enumerate().map(|(i, &c)| (c, i)).collect();
        for term in &self.terms {
            for &c in columns {
                if c < term.base
                    || c >= term.base + term.k * term.ids.len()
                    || (c - term.base) % term.k != 0
                {
                    continue;
                }
                for &(i, j, t) in &term.parameters {
                    l[(local[&(c + i)], local[&(c + j)])] = theta[t];
                }
            }
        }
        l
    }

    /// The profile for a design that splits into blocks. Each block's rows [R_Z Λ, R_X, R_y; I, 0, 0] are
    /// factored on their own; the rows left after eliminating the block's random columns are stacked and
    /// factored for the fixed effects. The quadratic forms are the dense profile's, so the objective and
    /// estimates agree with it to rounding.
    fn profile_blocks(&self, blocks: &[Block], theta: &[f64]) -> Result<Profile, StatsError> {
        let fixed_columns = self.p + 1;
        let mut factors = Vec::with_capacity(blocks.len());
        let mut rest_rows = 0;
        let mut parts = Vec::with_capacity(blocks.len());
        for block in blocks {
            let m = block.columns.len();
            let lambda = self.block_lambda(&block.columns, theta);
            let rows = block.r.nrows();
            let mut a = DMatrix::zeros(rows + m, m + fixed_columns);
            a.view_mut((0, 0), (rows, m))
                .copy_from(&(block.r.columns(0, m) * &lambda));
            a.view_mut((0, m), (rows, fixed_columns))
                .copy_from(&block.r.columns(m, fixed_columns));
            for j in 0..m {
                a[(rows + j, j)] = 1.0;
            }
            let r = a.qr().unpack_r();
            let kept = r.nrows();
            rest_rows += kept - m;
            parts.push(r.view((m, m), (kept - m, fixed_columns)).into_owned());
            factors.push((r, lambda));
        }
        let mut stacked = DMatrix::zeros(rest_rows, fixed_columns);
        let mut at = 0;
        for part in &parts {
            stacked
                .view_mut((at, 0), (part.nrows(), fixed_columns))
                .copy_from(part);
            at += part.nrows();
        }
        let fixed = stacked.qr().unpack_r();
        if fixed.nrows() < fixed_columns {
            return Err(err(
                ErrorKind::InvalidInput,
                "no positive finite residual variance",
            ));
        }
        let fixed_r = fixed.view((0, 0), (self.p, self.p)).into_owned();
        let beta = fixed_r
            .solve_upper_triangular(&fixed.view((0, self.p), (self.p, 1)).into_owned())
            .ok_or_else(|| {
                err(
                    ErrorKind::RankDeficiency,
                    "mixed-model fixed design is rank deficient",
                )
            })?;
        let rss = fixed[(self.p, self.p)].powi(2);
        let mut solution = DVector::zeros(self.q + self.p);
        solution.rows_mut(self.q, self.p).copy_from(&beta.column(0));
        let mut logdet_r = 0.0;
        let mut random = Vec::with_capacity(blocks.len());
        for (block, (r, lambda)) in blocks.iter().zip(factors) {
            let m = block.columns.len();
            let l = r.view((0, 0), (m, m)).into_owned();
            let rhs = r.view((0, m + self.p), (m, 1)) - r.view((0, m), (m, self.p)) * &beta;
            let u = l.solve_upper_triangular(&rhs).ok_or_else(|| {
                err(
                    ErrorKind::FactorizationFailure,
                    "cannot solve random-effect block",
                )
            })?;
            for (i, &c) in block.columns.iter().enumerate() {
                solution[c] = u[(i, 0)];
            }
            logdet_r += (0..m).map(|j| l[(j, j)].abs().ln()).sum::<f64>();
            random.push((l, lambda));
        }
        self.finish(solution, rss, fixed_r, logdet_r, Random::Blocks(random))
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

/// The random-effect factor: R_11 with R_11'R_11 = Λ'Z'WZΛ + I (the penalized random-effect block), and Λ.
enum Random {
    Dense {
        random_r: DMatrix<f64>,
        lambda: DMatrix<f64>,
    },
    /// Per block, in Data::blocks order: its R_11 block and its Λ block.
    Blocks(Vec<(DMatrix<f64>, DMatrix<f64>)>),
}
struct Profile {
    solution: DVector<f64>,
    fixed_r: DMatrix<f64>,
    variance: f64,
    objective: f64,
    random: Random,
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
    // Random-effect columns that share a row are connected; each connected set of columns, with the rows they
    // appear in, is a block that the penalized least-squares problem can factor on its own. One grouping term
    // has a block per group and nested terms a block per outer group. Crossed terms usually connect everything
    // into one block, which uses the dense summary below.
    let mut parent: Vec<usize> = (0..q).collect();
    fn root(parent: &mut [usize], mut c: usize) -> usize {
        while parent[c] != c {
            parent[c] = parent[parent[c]];
            c = parent[c];
        }
        c
    }
    let row_columns = |i: usize| {
        metadata
            .iter()
            .flat_map(move |m| (0..m.k).map(move |j| m.base + m.indices[i] * m.k + j))
    };
    for i in 0..n {
        let mut columns = row_columns(i);
        let first = columns.next().expect("each row has a random column");
        for c in columns {
            let (a, b) = (root(&mut parent, first), root(&mut parent, c));
            if a != b {
                parent[a.max(b)] = a.min(b);
            }
        }
    }
    let mut block_of = vec![usize::MAX; q];
    let mut block_columns: Vec<Vec<usize>> = Vec::new();
    for c in 0..q {
        let r = root(&mut parent, c);
        if block_of[r] == usize::MAX {
            block_of[r] = block_columns.len();
            block_columns.push(Vec::new());
        }
        block_of[c] = block_of[r];
        block_columns[block_of[c]].push(c);
    }
    let blocked = block_columns.len() > 1;
    // The dense summary is the general fallback; it is not lme4's sparse Cholesky. Fail explicitly rather than
    // allocate unbounded memory.
    if !blocked && (q > 512 || n.checked_mul(q + p + 1).is_none_or(|s| s > 50_000_000)) {
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
    let weighted_row = |i: usize, out: &mut dyn FnMut(usize, f64)| {
        let w = weights[i].sqrt();
        for (term, meta) in terms.iter().zip(&metadata) {
            for j in 0..meta.k {
                out(meta.base + meta.indices[i] * meta.k + j, w * term.z[i][j]);
            }
        }
    };
    let blocks = blocked.then(|| {
        let mut rows: Vec<Vec<usize>> = vec![Vec::new(); block_columns.len()];
        for i in 0..n {
            rows[block_of[row_columns(i).next().expect("each row has a random column")]].push(i);
        }
        block_columns
            .iter()
            .zip(&rows)
            .map(|(columns, rows)| {
                let m = columns.len();
                let local: HashMap<usize, usize> =
                    columns.iter().enumerate().map(|(l, &c)| (c, l)).collect();
                let mut a = DMatrix::zeros(rows.len(), m + p + 1);
                for (r, &i) in rows.iter().enumerate() {
                    weighted_row(i, &mut |c, v| a[(r, local[&c])] = v);
                    let w = weights[i].sqrt();
                    for j in 0..p {
                        a[(r, m + j)] = w * design[(i, j)] / scales[j];
                    }
                    a[(r, m + p)] = w * (y[i] - offset[i]);
                }
                Block {
                    columns: columns.clone(),
                    r: a.qr().unpack_r(),
                }
            })
            .collect::<Vec<_>>()
    });
    let mut augmented = DMatrix::zeros(if blocked { 0 } else { n }, q + p + 1);
    for i in 0..augmented.nrows() {
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
        blocks,
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
    let effects = match &fit.random {
        Random::Dense { lambda, .. } => lambda * fit.solution.rows(0, q),
        Random::Blocks(factors) => {
            let mut effects = DVector::zeros(q);
            for (block, (_, lambda)) in data.blocks.iter().flatten().zip(factors) {
                let u = DVector::from_iterator(
                    block.columns.len(),
                    block.columns.iter().map(|&c| fit.solution[c]),
                );
                for (v, &c) in (lambda * u).iter().zip(&block.columns) {
                    effects[c] = *v;
                }
            }
            effects
        }
    };
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
    // theta. With R_11'R_11 = L L', solve R_11' S = Λ' so the product is S'S. In blocks, R_11 and Λ are block
    // diagonal, so each group's covariance comes from its block's S.
    let spreads: Vec<(DMatrix<f64>, Vec<usize>)> = match &fit.random {
        Random::Dense { random_r, lambda } => vec![(
            random_r
                .transpose()
                .solve_lower_triangular(&lambda.transpose())
                .ok_or_else(|| {
                    err(
                        ErrorKind::FactorizationFailure,
                        "cannot solve random-effect conditional covariance factor",
                    )
                })?,
            (0..q).collect(),
        )],
        Random::Blocks(factors) => data
            .blocks
            .iter()
            .flatten()
            .zip(factors)
            .map(|(block, (l, lambda))| {
                l.transpose()
                    .solve_lower_triangular(&lambda.transpose())
                    .map(|spread| (spread, block.columns.clone()))
                    .ok_or_else(|| {
                        err(
                            ErrorKind::FactorizationFailure,
                            "cannot solve random-effect conditional covariance factor",
                        )
                    })
            })
            .collect::<Result<_, _>>()?,
    };
    let mut spread_of = vec![(0usize, 0usize); q];
    for (s, (_, columns)) in spreads.iter().enumerate() {
        for (local, &c) in columns.iter().enumerate() {
            spread_of[c] = (s, local);
        }
    }
    let cond_block = |base: usize, k: usize| -> Vec<Vec<f64>> {
        let (s, first) = spread_of[base];
        let spread = &spreads[s].0;
        (0..k)
            .map(|i| {
                (0..k)
                    .map(|j| fit.variance * spread.column(first + i).dot(&spread.column(first + j)))
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
