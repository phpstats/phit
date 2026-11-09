//! Two-stage least squares and homoskedastic diagnostics. See specs/cleanroom/iv.md.
use crate::{ErrorKind, OlsResult, StatsError, fit_ols};
use nalgebra::{DMatrix, DVector};
use statrs::distribution::{ChiSquared, ContinuousCDF, FisherSnedecor, StudentsT};

#[derive(Debug, Clone)]
pub struct FTest {
    pub statistic: f64,
    pub df1: usize,
    pub df2: usize,
    pub p_value: f64,
}
impl FTest {
    fn new(statistic: f64, df1: usize, df2: usize) -> Self {
        let p_value =
            FisherSnedecor::new(df1 as f64, df2 as f64).map_or(f64::NAN, |d| d.sf(statistic));
        Self {
            statistic,
            df1,
            df2,
            p_value,
        }
    }
}
#[derive(Debug, Clone)]
pub struct FirstStage {
    pub test: FTest,
    pub partial_r_squared: f64,
    pub shea_partial_r_squared: Option<f64>,
    pub sw: Option<FTest>,
    pub sw_df1: usize,
    pub sw_df2: usize,
    pub coefficients: Vec<f64>,
}
#[derive(Debug, Clone)]
pub struct Sargan {
    pub statistic: f64,
    pub df: usize,
    pub p_value: f64,
}
#[derive(Debug, Clone)]
pub struct IvResult {
    pub linear: OlsResult,
    pub first_stage: Vec<FirstStage>,
    pub wu_hausman: Option<FTest>,
    pub sargan: Option<Sargan>,
    pub warnings: Vec<String>,
}
fn rank_error(message: &str) -> StatsError {
    StatsError::new(ErrorKind::RankDeficiency, message)
}
fn rows(m: &DMatrix<f64>) -> Vec<Vec<f64>> {
    (0..m.nrows())
        .map(|i| m.row(i).iter().copied().collect())
        .collect()
}
/// Scaled Householder QR, with the same numerical rank criterion as OLS.
struct Projection {
    q: DMatrix<f64>,
    r: DMatrix<f64>,
    scales: Vec<f64>,
}
impl Projection {
    fn new(x: &DMatrix<f64>) -> Result<Self, StatsError> {
        let scales: Vec<_> = (0..x.ncols()).map(|j| x.column(j).norm()).collect();
        if scales.iter().any(|s| !s.is_finite() || *s == 0.0) || x.nrows() < x.ncols() {
            return Err(rank_error("IV design is rank deficient"));
        }
        let qr = DMatrix::from_fn(x.nrows(), x.ncols(), |i, j| x[(i, j)] / scales[j]).qr();
        let r = qr.r();
        let singular = r.singular_values();
        let tolerance = f64::EPSILON * x.nrows().max(x.ncols()) as f64 * singular.max();
        if singular.iter().any(|s| *s <= tolerance) {
            return Err(rank_error(
                "IV design is rank deficient at numerical precision",
            ));
        }
        Ok(Self {
            q: qr.q(),
            r,
            scales,
        })
    }
    fn project(&self, x: &DMatrix<f64>) -> DMatrix<f64> {
        &self.q * (self.q.transpose() * x)
    }
    fn solve(&self, y: &DVector<f64>) -> Result<DVector<f64>, StatsError> {
        let b = self
            .r
            .solve_upper_triangular(&(self.q.transpose() * y))
            .ok_or_else(|| rank_error("IV triangular solve failed"))?;
        Ok(DVector::from_iterator(
            b.len(),
            b.iter().zip(&self.scales).map(|(b, s)| b / s),
        ))
    }
    fn inverse_gram(&self) -> Result<DMatrix<f64>, StatsError> {
        let g = crate::observations::inverse_gram(&self.r)
            .ok_or_else(|| rank_error("IV triangular inverse failed"))?;
        Ok(DMatrix::from_fn(g.nrows(), g.ncols(), |i, j| {
            g[(i, j)] / self.scales[i] / self.scales[j]
        }))
    }
}
fn residualize(x: &DMatrix<f64>, controls: &DMatrix<f64>) -> Result<DMatrix<f64>, StatsError> {
    if controls.ncols() == 0 {
        return Ok(x.clone());
    }
    Ok(x - Projection::new(controls)?.project(x))
}
fn select(x: &DMatrix<f64>, indices: &[usize]) -> DMatrix<f64> {
    DMatrix::from_fn(x.nrows(), indices.len(), |i, j| x[(i, indices[j])])
}

/// X is the structural design, Z contains exogenous and excluded instrument columns.
/// Endogenous indices refer to X, and weights are positive prior weights.
pub fn fit_iv(
    x: Vec<Vec<f64>>,
    y: Vec<f64>,
    z: Vec<Vec<f64>>,
    endogenous: &[usize],
    weights: Vec<f64>,
    intercept: bool,
) -> Result<IvResult, StatsError> {
    let n = x.len();
    let k = x.first().map_or(0, Vec::len);
    let l = z.first().map_or(0, Vec::len);
    if n == 0
        || k == 0
        || z.len() != n
        || y.len() != n
        || weights.len() != n
        || x.iter().any(|r| r.len() != k)
        || z.iter().any(|r| r.len() != l)
        || endogenous.is_empty()
        || endogenous.iter().any(|j| *j >= k)
        || endogenous
            .iter()
            .enumerate()
            .any(|(i, j)| endogenous[..i].contains(j))
    {
        return Err(StatsError::new(
            ErrorKind::DimensionMismatch,
            "IV matrices, response, weights, and endogenous indices must align",
        ));
    }
    if x.iter()
        .flatten()
        .chain(z.iter().flatten())
        .chain(&y)
        .chain(&weights)
        .any(|v| !v.is_finite())
        || weights.iter().any(|w| *w <= 0.0)
    {
        return Err(StatsError::new(
            ErrorKind::InvalidInput,
            "IV inputs must be finite and weights positive",
        ));
    }
    if l < k {
        return Err(rank_error(
            "under-identified IV model: fewer instruments than regressors",
        ));
    }
    if n <= l || n <= k {
        return Err(StatsError::new(
            ErrorKind::DimensionMismatch,
            "IV requires positive instrument and structural residual degrees of freedom",
        ));
    }
    let raw_x = DMatrix::from_row_iterator(n, k, x.into_iter().flatten());
    let wx = DMatrix::from_fn(n, k, |i, j| raw_x[(i, j)] * weights[i].sqrt());
    let wz = DMatrix::from_fn(n, l, |i, j| z[i][j] * weights[i].sqrt());
    let wy = DVector::from_iterator(n, (0..n).map(|i| y[i] * weights[i].sqrt()));
    let projection = Projection::new(&wz)?;
    let projected = projection.project(&wx);
    let second = Projection::new(&projected)?;
    let beta = second.solve(&wy)?;
    let fitted = &raw_x * &beta;
    let residuals = DVector::from_vec(y.clone()) - &fitted;
    let rss = (0..n)
        .map(|i| weights[i] * residuals[i].powi(2))
        .sum::<f64>();
    let df = n - k;
    let variance = rss / df as f64;
    let covariance = second.inverse_gram()? * variance;
    let t = StudentsT::new(0., 1., df as f64).unwrap();
    let critical = crate::ols::t_critical_95(&t);
    let std_errors: Vec<_> = (0..k).map(|j| covariance[(j, j)].sqrt()).collect();
    let t_statistics: Vec<_> = (0..k).map(|j| beta[j] / std_errors[j]).collect();
    let p_values = t_statistics
        .iter()
        .map(|v| {
            if v.is_nan() {
                f64::NAN
            } else {
                2. * t.sf(v.abs())
            }
        })
        .collect();
    let conf_int = (0..k)
        .map(|j| {
            [
                beta[j] - critical * std_errors[j],
                beta[j] + critical * std_errors[j],
            ]
        })
        .collect();
    let mean = if intercept {
        (0..n).map(|i| weights[i] * y[i]).sum::<f64>() / weights.iter().sum::<f64>()
    } else {
        0.
    };
    let tss = (0..n)
        .map(|i| weights[i] * (y[i] - mean).powi(2))
        .sum::<f64>();
    let r_squared = 1. - rss / tss;
    let linear = OlsResult {
        coefficients: beta.iter().copied().collect(),
        covariance: rows(&covariance),
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
        adj_r_squared: 1. - (1. - r_squared) * (n - usize::from(intercept)) as f64 / df as f64,
        df_residual: df,
        log_likelihood: -0.5
            * n as f64
            * ((2. * std::f64::consts::PI).ln() + (rss / n as f64).ln() + 1.)
            + 0.5 * weights.iter().map(|w| w.ln()).sum::<f64>(),
    };
    if linear
        .coefficients
        .iter()
        .chain(&linear.fitted)
        .chain(&linear.residuals)
        .chain(linear.covariance.iter().flatten())
        .any(|v| !v.is_finite())
        || !rss.is_finite()
        || !tss.is_finite()
    {
        return Err(StatsError::new(
            ErrorKind::Nonfinite,
            "IV calculation exceeds floating-point range",
        ));
    }
    let exog: Vec<_> = (0..k).filter(|j| !endogenous.contains(j)).collect();
    let c = select(&wx, &exog);
    let d = select(&wx, endogenous);
    let dhat = select(&projected, endogenous);
    let v = &d - &dhat;
    let q = l - exog.len();
    let m = endogenous.len();
    let mut warnings = Vec::new();
    let mut first_stage = Vec::new();
    for (j, index) in endogenous.iter().enumerate() {
        let dj = d.columns(j, 1).into_owned();
        let marginal = residualize(&dj, &c)?.norm_squared();
        let error = v.column(j).norm_squared();
        let partial = (1. - error / marginal).clamp(0., 1.);
        let test = FTest::new(
            ((marginal - error).max(0.) / q as f64) / (error / (n - l) as f64),
            q,
            n - l,
        );
        let others: Vec<_> = (0..m).filter(|h| *h != j).collect();
        let mut shea = None;
        let sw_df1 = q - m + 1;
        let sw_df2 = n - l;
        let sw = if m == 1 {
            Some(test.clone())
        } else {
            // S&W (2016): remove other fitted endogenous variables, then use
            // reduced-form residual v_j - V_-j delta in the denominator.
            let mut control = c.clone().insert_columns(c.ncols(), m - 1, 0.);
            control
                .columns_mut(c.ncols(), m - 1)
                .copy_from(&select(&dhat, &others));
            match Projection::new(&control).and_then(|p| p.solve(&d.column(j).into_owned())) {
                Ok(delta) => {
                    let conditional = &dj - &control * &delta;
                    let signal = (projection.q.transpose() * &conditional).norm_squared();
                    let correction =
                        DVector::from_iterator(m - 1, (0..m - 1).map(|h| delta[c.ncols() + h]));
                    let noise = v.column(j) - select(&v, &others) * correction;
                    let f = (signal / sw_df1 as f64) / (noise.norm_squared() / sw_df2 as f64);
                    // Shea: ratio of residual variation after partialling the
                    // other actual and fitted endogenous variables respectively.
                    let mut actual = c.clone().insert_columns(c.ncols(), m - 1, 0.);
                    actual
                        .columns_mut(c.ncols(), m - 1)
                        .copy_from(&select(&d, &others));
                    let numerator = residualize(&dhat.columns(j, 1).into_owned(), &control)
                        .map(|r| r.norm_squared());
                    let denominator = residualize(&dj, &actual).map(|r| r.norm_squared());
                    if let (Ok(a), Ok(b)) = (numerator, denominator) {
                        let r = a / b;
                        if r.is_finite() {
                            shea = Some(r.clamp(0., 1.));
                        }
                    }
                    if f.is_finite() {
                        Some(FTest::new(f, sw_df1, sw_df2))
                    } else {
                        None
                    }
                }
                Err(_) => None,
            }
        };
        if sw.is_none() {
            warnings.push(format!("Sanderson-Windmeijer F unavailable for endogenous column {}: singular conditional regression or undefined variance", index));
        }
        first_stage.push(FirstStage {
            test,
            partial_r_squared: partial,
            shea_partial_r_squared: shea,
            sw,
            sw_df1,
            sw_df2,
            coefficients: projection
                .solve(&d.column(j).into_owned())?
                .iter()
                .copied()
                .collect(),
        });
    }
    let mut augmented = wx.clone().insert_columns(k, m, 0.);
    augmented.columns_mut(k, m).copy_from(&v);
    let wu_hausman = if n > k + m {
        let restricted = fit_ols(rows(&wx), wy.iter().copied().collect());
        let unrestricted = fit_ols(rows(&augmented), wy.iter().copied().collect());
        match (restricted, unrestricted) {
            (Ok(a), Ok(b)) => Some(FTest::new(
                ((a.rss - b.rss).max(0.) / m as f64) / (b.rss / (n - k - m) as f64),
                m,
                n - k - m,
            )),
            _ => None,
        }
    } else {
        None
    };
    if wu_hausman.is_none() {
        warnings.push("Wu-Hausman test unavailable: insufficient degrees of freedom or singular augmented regression".into());
    }
    let sargan = if l > k {
        let wr = DMatrix::from_fn(n, 1, |i, _| residuals[i] * weights[i].sqrt());
        let unexplained = (&wr - projection.project(&wr)).norm_squared();
        let statistic = n as f64 * (1. - unexplained / rss).clamp(0., 1.);
        Some(Sargan {
            statistic,
            df: l - k,
            p_value: ChiSquared::new((l - k) as f64).unwrap().sf(statistic),
        })
    } else {
        None
    };
    Ok(IvResult {
        linear,
        first_stage,
        wu_hausman,
        sargan,
        warnings,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_duplicate_endogenous_indices() {
        let x = vec![vec![1., 1.], vec![1., 2.], vec![1., 3.], vec![1., 4.]];
        let error = fit_iv(
            x.clone(),
            vec![1., 2., 3., 4.],
            x,
            &[1, 1],
            vec![1.; 4],
            true,
        )
        .unwrap_err();
        assert_eq!(error.kind(), ErrorKind::DimensionMismatch);
    }
    #[test]
    fn unavailable_conditional_variance_has_null_sw_and_warning() {
        let z = vec![
            vec![1., 0., 0.],
            vec![0., 1., 0.],
            vec![0., 0., 1.],
            vec![0., 0., 0.],
            vec![0., 0., 0.],
        ];
        let x = z.iter().map(|r| r[..2].to_vec()).collect();
        let r = fit_iv(x, vec![1., 2., 3., 4., 5.], z, &[0, 1], vec![1.; 5], false).unwrap();
        assert!(r.first_stage.iter().all(|s| s.sw.is_none()));
        assert_eq!(r.first_stage[0].sw_df1, 2);
        assert_eq!(r.first_stage[0].sw_df2, 2);
        assert!(
            r.warnings
                .iter()
                .any(|w| w.contains("Sanderson-Windmeijer"))
        );
    }
    #[test]
    fn structural_residuals_define_variance_even_if_second_stage_is_perfect() {
        let z = vec![vec![1., -1.], vec![1., -1.], vec![1., 1.], vec![1., 1.]];
        let x = vec![vec![1., -2.], vec![1., 0.], vec![1., 0.], vec![1., 2.]];
        let r = fit_iv(x, vec![-1., -1., 1., 1.], z, &[1], vec![1.; 4], true).unwrap();
        assert!((r.linear.coefficients[1] - 1.).abs() < 1e-12);
        assert!((r.linear.residual_variance - 2.).abs() < 1e-12);
        assert!((r.linear.covariance[1][1] - 0.5).abs() < 1e-12);
        assert_eq!(
            r.first_stage[0].sw.as_ref().unwrap().statistic,
            r.first_stage[0].test.statistic
        );
    }
}
