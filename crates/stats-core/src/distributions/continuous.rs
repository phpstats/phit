//! Continuous distributions. Methods expect a non-NaN x (±∞ allowed) and a
//! quantile probability in [0, 1]; the public `checked_*` helpers enforce that.
use super::quantile::{self, LogTails};
use super::special::{
    LN_SQRT_2PI, TINY, ln_beta_lower_fraction, ln_gamma_lower_series, ln_gamma_upper_fraction,
    ln_one_minus_exp_neg, ln_poisson_raw, log_tail, normal_log_lower_tail, softplus,
};
use super::{beta_inc, gamma_inc};
use super::{finite, positive};
use crate::StatsError;
use cdflib::special::internal::algdiv;
use cdflib::special::{beta_log, cumnor, dinvnr, dt1, gamma_log};

/// Replace a probability below TINY by exp(ln p) from a log-scale tail, which
/// stays accurate where a transformed argument has underflowed.
fn refine(value: f64, ln_value: impl FnOnce() -> f64) -> f64 {
    if value >= TINY {
        return value;
    }
    let ln_value = ln_value();
    if ln_value.is_nan() {
        value
    } else {
        ln_value.exp()
    }
}

fn std_logpdf(z: f64) -> f64 {
    -0.5 * z * z - LN_SQRT_2PI
}
fn std_tails(z: f64) -> (f64, f64) {
    if z == f64::INFINITY {
        (1.0, 0.0)
    } else if z == f64::NEG_INFINITY {
        (0.0, 1.0)
    } else {
        cumnor(z)
    }
}
fn std_log_cdf(z: f64) -> f64 {
    let (lower, upper) = std_tails(z);
    log_tail(lower, upper, || normal_log_lower_tail(z))
}
/// Standard normal quantile, polished on the log scale from CDFLIB's dinvnr.
pub(crate) fn std_quantile(p: f64) -> f64 {
    if p == 0.5 {
        return 0.0;
    }
    if p > 0.5 {
        return -std_quantile(1.0 - p);
    }
    let start = if p > 0.0 { dinvnr(p, 1.0 - p) } else { 0.0 };
    let tails = LogTails {
        log_cdf: &std_log_cdf,
        log_sf: &|z| std_log_cdf(-z),
        log_pdf: &std_logpdf,
    };
    quantile::continuous(p, start, f64::NEG_INFINITY, f64::INFINITY, &tails)
}

/// Normal distribution with mean and standard deviation.
#[derive(Debug, Clone, Copy)]
pub struct Normal {
    mean: f64,
    sd: f64,
}
impl Normal {
    pub fn new(mean: f64, sd: f64) -> Result<Self, StatsError> {
        Ok(Self {
            mean: finite("mean", mean)?,
            sd: positive("sd", sd)?,
        })
    }
    fn z(&self, x: f64) -> f64 {
        (x - self.mean) / self.sd
    }
    pub fn pdf(&self, x: f64) -> f64 {
        if self.sd == 1.0 {
            crate::numeric::standard_normal_density(self.z(x))
        } else {
            // Apply scale on the log scale before exponentiating, so a tiny
            // standard density is not lost before division by a tiny SD.
            self.logpdf(x).exp()
        }
    }
    pub fn logpdf(&self, x: f64) -> f64 {
        std_logpdf(self.z(x)) - self.sd.ln()
    }
    pub fn cdf(&self, x: f64) -> f64 {
        std_tails(self.z(x)).0
    }
    pub fn sf(&self, x: f64) -> f64 {
        std_tails(self.z(x)).1
    }
    pub fn logcdf(&self, x: f64) -> f64 {
        std_log_cdf(self.z(x))
    }
    pub fn logsf(&self, x: f64) -> f64 {
        std_log_cdf(-self.z(x))
    }
    pub fn quantile(&self, p: f64) -> f64 {
        self.mean + self.sd * std_quantile(p)
    }
}

/// Log-normal distribution: ln X ~ Normal(meanlog, sdlog).
#[derive(Debug, Clone, Copy)]
pub struct LogNormal {
    meanlog: f64,
    sdlog: f64,
}
impl LogNormal {
    pub fn new(meanlog: f64, sdlog: f64) -> Result<Self, StatsError> {
        Ok(Self {
            meanlog: finite("meanlog", meanlog)?,
            sdlog: positive("sdlog", sdlog)?,
        })
    }
    fn z(&self, x: f64) -> f64 {
        if x <= 0.0 {
            f64::NEG_INFINITY
        } else {
            (x.ln() - self.meanlog) / self.sdlog
        }
    }
    pub fn pdf(&self, x: f64) -> f64 {
        if x <= 0.0 || x.is_infinite() {
            return 0.0;
        }
        // Form the complete log density before exponentiating: the standard
        // normal density may underflow even when division by x restores a
        // representable log-normal density.
        self.logpdf(x).exp()
    }
    pub fn logpdf(&self, x: f64) -> f64 {
        if x <= 0.0 || x.is_infinite() {
            return f64::NEG_INFINITY;
        }
        std_logpdf(self.z(x)) - self.sdlog.ln() - x.ln()
    }
    pub fn cdf(&self, x: f64) -> f64 {
        std_tails(self.z(x)).0
    }
    pub fn sf(&self, x: f64) -> f64 {
        std_tails(self.z(x)).1
    }
    pub fn logcdf(&self, x: f64) -> f64 {
        std_log_cdf(self.z(x))
    }
    pub fn logsf(&self, x: f64) -> f64 {
        std_log_cdf(-self.z(x))
    }
    pub fn quantile(&self, p: f64) -> f64 {
        (self.meanlog + self.sdlog * std_quantile(p)).exp()
    }
}

/// Logistic distribution with location and scale.
#[derive(Debug, Clone, Copy)]
pub struct Logistic {
    location: f64,
    scale: f64,
}
impl Logistic {
    pub fn new(location: f64, scale: f64) -> Result<Self, StatsError> {
        Ok(Self {
            location: finite("location", location)?,
            scale: positive("scale", scale)?,
        })
    }
    fn z(&self, x: f64) -> f64 {
        (x - self.location) / self.scale
    }
    pub fn pdf(&self, x: f64) -> f64 {
        let e = (-self.z(x).abs()).exp();
        e / (self.scale * (1.0 + e) * (1.0 + e))
    }
    pub fn logpdf(&self, x: f64) -> f64 {
        let a = self.z(x).abs();
        -a - 2.0 * (-a).exp().ln_1p() - self.scale.ln()
    }
    pub fn cdf(&self, x: f64) -> f64 {
        logistic_cdf(self.z(x))
    }
    pub fn sf(&self, x: f64) -> f64 {
        logistic_cdf(-self.z(x))
    }
    pub fn logcdf(&self, x: f64) -> f64 {
        -softplus(-self.z(x))
    }
    pub fn logsf(&self, x: f64) -> f64 {
        -softplus(self.z(x))
    }
    pub fn quantile(&self, p: f64) -> f64 {
        let logit = if p <= 0.5 {
            p.ln() - (-p).ln_1p()
        } else {
            let q = 1.0 - p;
            (-q).ln_1p() - q.ln()
        };
        self.location + self.scale * logit
    }
}
fn logistic_cdf(z: f64) -> f64 {
    if z >= 0.0 {
        1.0 / (1.0 + (-z).exp())
    } else {
        let e = z.exp();
        e / (1.0 + e)
    }
}

/// Exponential distribution with rate λ (mean 1/λ).
#[derive(Debug, Clone, Copy)]
pub struct Exponential {
    rate: f64,
}
impl Exponential {
    pub fn new(rate: f64) -> Result<Self, StatsError> {
        Ok(Self {
            rate: positive("rate", rate)?,
        })
    }
    pub fn pdf(&self, x: f64) -> f64 {
        if x < 0.0 {
            0.0
        } else {
            self.rate * (-self.rate * x).exp()
        }
    }
    pub fn logpdf(&self, x: f64) -> f64 {
        if x < 0.0 {
            f64::NEG_INFINITY
        } else {
            self.rate.ln() - self.rate * x
        }
    }
    pub fn cdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            0.0
        } else {
            -(-self.rate * x).exp_m1()
        }
    }
    pub fn sf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            1.0
        } else {
            (-self.rate * x).exp()
        }
    }
    pub fn logcdf(&self, x: f64) -> f64 {
        if x <= 0.0 {
            f64::NEG_INFINITY
        } else {
            ln_one_minus_exp_neg(self.rate * x)
        }
    }
    pub fn logsf(&self, x: f64) -> f64 {
        if x <= 0.0 { 0.0 } else { -self.rate * x }
    }
    pub fn quantile(&self, p: f64) -> f64 {
        if p <= 0.5 {
            -(-p).ln_1p() / self.rate
        } else {
            -(1.0 - p).ln() / self.rate
        }
    }
}

/// Student's t distribution with df degrees of freedom (location 0, scale 1).
#[derive(Debug, Clone, Copy)]
pub struct StudentT {
    df: f64,
}
// Beta-function arguments for the tail P(T > |x|) = I_h(df/2, 1/2) / 2.
struct TArgs {
    h: f64,
    y: f64,
    ln_h: f64,
    ln_y: f64,
}
impl StudentT {
    pub fn new(df: f64) -> Result<Self, StatsError> {
        Ok(Self {
            df: positive("df", df)?,
        })
    }
    fn args(&self, x: f64) -> TArgs {
        let u = x.abs() / self.df.sqrt();
        let r = u * u;
        if r.is_finite() {
            TArgs {
                h: 1.0 / (1.0 + r),
                y: r / (1.0 + r),
                ln_h: -r.ln_1p(),
                ln_y: -(1.0 / r).ln_1p(),
            }
        } else {
            // |x|/√df beyond 1e154: work with ln(|x|/√df) to avoid overflow.
            let ln_u = x.abs().ln() - 0.5 * self.df.ln();
            let h = (-2.0 * ln_u).exp();
            TArgs {
                h,
                y: 1.0,
                ln_h: -2.0 * ln_u,
                ln_y: -h,
            }
        }
    }
    fn ln_small_tail(&self, a: &TArgs) -> f64 {
        -std::f64::consts::LN_2 + ln_beta_lower_fraction(0.5 * self.df, 0.5, a.h, a.ln_h, a.ln_y)
    }
    // (P(T ≤ x), P(T > x)) with the small tail computed directly.
    fn tails(&self, x: f64) -> (f64, f64) {
        let a = self.args(x);
        let (i, one_minus_i) = if a.h == 0.0 {
            (0.0, 1.0)
        } else {
            beta_inc(0.5 * self.df, 0.5, a.h, a.y)
        };
        let small = refine(0.5 * i, || self.ln_small_tail(&a));
        let big = one_minus_i + 0.5 * i;
        if x <= 0.0 { (small, big) } else { (big, small) }
    }
    pub fn pdf(&self, x: f64) -> f64 {
        self.logpdf(x).exp()
    }
    pub fn logpdf(&self, x: f64) -> f64 {
        let half = 0.5 * self.df;
        // ln Γ((df + 1)/2) - ln Γ(df/2) without cancellation for large df.
        let ratio = if half >= 8.0 {
            -algdiv(0.5, half)
        } else {
            gamma_log(half + 0.5) - gamma_log(half)
        };
        let a = self.args(x);
        ratio - 0.5 * (std::f64::consts::PI * self.df).ln() + (half + 0.5) * a.ln_h
    }
    pub fn cdf(&self, x: f64) -> f64 {
        self.tails(x).0
    }
    pub fn sf(&self, x: f64) -> f64 {
        self.tails(x).1
    }
    pub fn logcdf(&self, x: f64) -> f64 {
        let (lower, upper) = self.tails(x);
        log_tail(lower, upper, || self.ln_small_tail(&self.args(x)))
    }
    pub fn logsf(&self, x: f64) -> f64 {
        let (lower, upper) = self.tails(x);
        log_tail(upper, lower, || self.ln_small_tail(&self.args(x)))
    }
    pub fn quantile(&self, p: f64) -> f64 {
        if p == 0.5 {
            return 0.0;
        }
        if p > 0.5 {
            return -self.quantile(1.0 - p);
        }
        let start = if p > 0.0 {
            dt1(p, 1.0 - p, self.df)
        } else {
            0.0
        };
        let tails = LogTails {
            log_cdf: &|x| self.logcdf(x),
            log_sf: &|x| self.logsf(x),
            log_pdf: &|x| self.logpdf(x),
        };
        quantile::continuous(p, start, f64::NEG_INFINITY, f64::INFINITY, &tails)
    }
}

/// Gamma distribution with shape k and scale θ (mean kθ).
#[derive(Debug, Clone, Copy)]
pub struct Gamma {
    shape: f64,
    scale: f64,
}
impl Gamma {
    pub fn new(shape: f64, scale: f64) -> Result<Self, StatsError> {
        Ok(Self {
            shape: positive("shape", shape)?,
            scale: positive("scale", scale)?,
        })
    }
    // Standard (scale 1) pieces in y = x / scale.
    fn std_logpdf(&self, y: f64) -> f64 {
        let a = self.shape;
        if y < 0.0 || y.is_infinite() {
            return f64::NEG_INFINITY;
        }
        if y == 0.0 {
            return if a < 1.0 {
                f64::INFINITY
            } else if a == 1.0 {
                0.0
            } else {
                f64::NEG_INFINITY
            };
        }
        if y < 1e-100 {
            (a - 1.0) * y.ln() - y - gamma_log(a)
        } else if a < 1.0 {
            ln_poisson_raw(a, y) + a.ln() - y.ln()
        } else {
            ln_poisson_raw(a - 1.0, y)
        }
    }
    fn std_exact(y: f64) -> Option<(f64, f64)> {
        if y <= 0.0 {
            Some((0.0, 1.0))
        } else if y.is_infinite() {
            Some((1.0, 0.0))
        } else {
            None
        }
    }
    fn std_tails(&self, y: f64) -> (f64, f64) {
        if let Some(exact) = Self::std_exact(y) {
            return exact;
        }
        let (p, q) = gamma_inc(self.shape, y);
        (
            refine(p, || ln_gamma_lower_series(self.shape, y)),
            refine(q, || ln_gamma_upper_fraction(self.shape, y)),
        )
    }
    fn std_logcdf(&self, y: f64) -> f64 {
        if let Some((p, _)) = Self::std_exact(y) {
            return p.ln();
        }
        let (p, q) = self.std_tails(y);
        log_tail(p, q, || ln_gamma_lower_series(self.shape, y))
    }
    fn std_logsf(&self, y: f64) -> f64 {
        if let Some((_, q)) = Self::std_exact(y) {
            return q.ln();
        }
        let (p, q) = self.std_tails(y);
        log_tail(q, p, || ln_gamma_upper_fraction(self.shape, y))
    }
    pub fn pdf(&self, x: f64) -> f64 {
        self.logpdf(x).exp()
    }
    pub fn logpdf(&self, x: f64) -> f64 {
        self.std_logpdf(x / self.scale) - self.scale.ln()
    }
    pub fn cdf(&self, x: f64) -> f64 {
        self.std_tails(x / self.scale).0
    }
    pub fn sf(&self, x: f64) -> f64 {
        self.std_tails(x / self.scale).1
    }
    pub fn logcdf(&self, x: f64) -> f64 {
        self.std_logcdf(x / self.scale)
    }
    pub fn logsf(&self, x: f64) -> f64 {
        self.std_logsf(x / self.scale)
    }
    pub fn quantile(&self, p: f64) -> f64 {
        let a = self.shape;
        // Wilson–Hilferty start, or the small-x power law when it is not positive.
        let c = 1.0 / (9.0 * a);
        let w = 1.0 - c + std_quantile(p) * c.sqrt();
        let start = if w > 0.0 {
            a * w * w * w
        } else {
            ((p.ln() + gamma_log(a + 1.0)) / a).exp()
        };
        let tails = LogTails {
            log_cdf: &|y| self.std_logcdf(y),
            log_sf: &|y| self.std_logsf(y),
            log_pdf: &|y| self.std_logpdf(y),
        };
        self.scale * quantile::continuous(p, start, 0.0, f64::INFINITY, &tails)
    }
}

/// Chi-square distribution with df degrees of freedom: Gamma(df/2, scale 2).
#[derive(Debug, Clone, Copy)]
pub struct ChiSquare {
    gamma: Gamma,
}
impl ChiSquare {
    pub fn new(df: f64) -> Result<Self, StatsError> {
        Ok(Self {
            gamma: Gamma::new(0.5 * positive("df", df)?, 2.0)?,
        })
    }
    pub fn pdf(&self, x: f64) -> f64 {
        self.gamma.pdf(x)
    }
    pub fn logpdf(&self, x: f64) -> f64 {
        self.gamma.logpdf(x)
    }
    pub fn cdf(&self, x: f64) -> f64 {
        self.gamma.cdf(x)
    }
    pub fn sf(&self, x: f64) -> f64 {
        self.gamma.sf(x)
    }
    pub fn logcdf(&self, x: f64) -> f64 {
        self.gamma.logcdf(x)
    }
    pub fn logsf(&self, x: f64) -> f64 {
        self.gamma.logsf(x)
    }
    pub fn quantile(&self, p: f64) -> f64 {
        self.gamma.quantile(p)
    }
}

/// Beta distribution with shape parameters α and β.
#[derive(Debug, Clone, Copy)]
pub struct Beta {
    alpha: f64,
    beta: f64,
}
impl Beta {
    pub fn new(alpha: f64, beta: f64) -> Result<Self, StatsError> {
        Ok(Self {
            alpha: positive("alpha", alpha)?,
            beta: positive("beta", beta)?,
        })
    }
    fn ln_lower(&self, x: f64) -> f64 {
        ln_beta_lower_fraction(self.alpha, self.beta, x, x.ln(), (-x).ln_1p())
    }
    fn ln_upper(&self, x: f64) -> f64 {
        ln_beta_lower_fraction(self.beta, self.alpha, 1.0 - x, (-x).ln_1p(), x.ln())
    }
    fn exact(x: f64) -> Option<(f64, f64)> {
        if x <= 0.0 {
            Some((0.0, 1.0))
        } else if x >= 1.0 {
            Some((1.0, 0.0))
        } else {
            None
        }
    }
    fn tails(&self, x: f64) -> (f64, f64) {
        if let Some(exact) = Self::exact(x) {
            return exact;
        }
        let (lower, upper) = beta_inc(self.alpha, self.beta, x, 1.0 - x);
        (
            refine(lower, || self.ln_lower(x)),
            refine(upper, || self.ln_upper(x)),
        )
    }
    pub fn pdf(&self, x: f64) -> f64 {
        self.logpdf(x).exp()
    }
    pub fn logpdf(&self, x: f64) -> f64 {
        let (a, b) = (self.alpha, self.beta);
        if !(0.0..=1.0).contains(&x) {
            return f64::NEG_INFINITY;
        }
        let edge = |shape: f64, other: f64| {
            if shape < 1.0 {
                f64::INFINITY
            } else if shape == 1.0 {
                other.ln()
            } else {
                f64::NEG_INFINITY
            }
        };
        if x == 0.0 {
            return edge(a, b);
        }
        if x == 1.0 {
            return edge(b, a);
        }
        if a <= 2.0 || b <= 2.0 {
            (a - 1.0) * x.ln() + (b - 1.0) * (-x).ln_1p() - beta_log(a, b)
        } else {
            (a + b - 1.0).ln() + super::special::ln_binomial_raw(a - 1.0, a + b - 2.0, x, 1.0 - x)
        }
    }
    pub fn cdf(&self, x: f64) -> f64 {
        self.tails(x).0
    }
    pub fn sf(&self, x: f64) -> f64 {
        self.tails(x).1
    }
    pub fn logcdf(&self, x: f64) -> f64 {
        if let Some((lower, _)) = Self::exact(x) {
            return lower.ln();
        }
        let (lower, upper) = self.tails(x);
        log_tail(lower, upper, || self.ln_lower(x))
    }
    pub fn logsf(&self, x: f64) -> f64 {
        if let Some((_, upper)) = Self::exact(x) {
            return upper.ln();
        }
        let (lower, upper) = self.tails(x);
        log_tail(upper, lower, || self.ln_upper(x))
    }
    pub fn quantile(&self, p: f64) -> f64 {
        let (a, b) = (self.alpha, self.beta);
        let mean = a / (a + b);
        let start = if p < 0.01 {
            ((p.ln() + a.ln() + beta_log(a, b)) / a).exp().min(mean)
        } else if p > 0.99 {
            1.0 - ((((1.0 - p).ln() + b.ln() + beta_log(a, b)) / b).exp()).min(1.0 - mean)
        } else {
            let sd = (a * b / ((a + b) * (a + b) * (a + b + 1.0))).sqrt();
            (mean + sd * std_quantile(p)).clamp(1e-3 * mean, 1.0 - 1e-3 * (1.0 - mean))
        };
        let tails = LogTails {
            log_cdf: &|x| self.logcdf(x),
            log_sf: &|x| self.logsf(x),
            log_pdf: &|x| self.logpdf(x),
        };
        quantile::continuous(p, start, 0.0, 1.0, &tails)
    }
}

/// F distribution with numerator df1 and denominator df2 degrees of freedom.
#[derive(Debug, Clone, Copy)]
pub struct FisherF {
    df1: f64,
    df2: f64,
}
// Beta arguments u = df1 x/(df1 x + df2) and v = 1 - u, with their logs.
struct FArgs {
    u: f64,
    v: f64,
    ln_u: f64,
    ln_v: f64,
}
impl FisherF {
    pub fn new(df1: f64, df2: f64) -> Result<Self, StatsError> {
        Ok(Self {
            df1: positive("df1", df1)?,
            df2: positive("df2", df2)?,
        })
    }
    fn args(&self, x: f64) -> FArgs {
        let r = self.df1 / self.df2 * x;
        if r.is_finite() {
            FArgs {
                u: r / (1.0 + r),
                v: 1.0 / (1.0 + r),
                ln_u: -(1.0 / r).ln_1p(),
                ln_v: -r.ln_1p(),
            }
        } else {
            let v = self.df2 / self.df1 / x;
            FArgs {
                u: 1.0,
                v,
                ln_u: -v,
                ln_v: (self.df2 / self.df1).ln() - x.ln(),
            }
        }
    }
    fn ln_lower(&self, a: &FArgs) -> f64 {
        ln_beta_lower_fraction(0.5 * self.df1, 0.5 * self.df2, a.u, a.ln_u, a.ln_v)
    }
    fn ln_upper(&self, a: &FArgs) -> f64 {
        ln_beta_lower_fraction(0.5 * self.df2, 0.5 * self.df1, a.v, a.ln_v, a.ln_u)
    }
    fn exact(x: f64) -> Option<(f64, f64)> {
        if x <= 0.0 {
            Some((0.0, 1.0))
        } else if x.is_infinite() {
            Some((1.0, 0.0))
        } else {
            None
        }
    }
    fn tails(&self, x: f64) -> (f64, f64) {
        if let Some(exact) = Self::exact(x) {
            return exact;
        }
        let a = self.args(x);
        let (lower, upper) = if a.v == 0.0 {
            (1.0, 0.0)
        } else {
            beta_inc(0.5 * self.df1, 0.5 * self.df2, a.u, a.v)
        };
        (
            refine(lower, || self.ln_lower(&a)),
            refine(upper, || self.ln_upper(&a)),
        )
    }
    pub fn pdf(&self, x: f64) -> f64 {
        self.logpdf(x).exp()
    }
    pub fn logpdf(&self, x: f64) -> f64 {
        let (m, n) = (self.df1, self.df2);
        if x < 0.0 || x.is_infinite() {
            return f64::NEG_INFINITY;
        }
        if x == 0.0 {
            return if m < 2.0 {
                f64::INFINITY
            } else if m == 2.0 {
                0.0
            } else {
                f64::NEG_INFINITY
            };
        }
        let a = self.args(x);
        if a.v < 1e-100 {
            // Far right tail: the direct formula has no cancellation here.
            return 0.5 * m * (m / n).ln() + (0.5 * m - 1.0) * x.ln() + 0.5 * (m + n) * a.ln_v
                - beta_log(0.5 * m, 0.5 * n);
        }
        use super::special::ln_binomial_raw;
        if m >= 2.0 {
            (0.5 * m * a.v).ln() + ln_binomial_raw(0.5 * m - 1.0, 0.5 * (m + n) - 1.0, a.u, a.v)
        } else {
            (m * m * a.v / (2.0 * a.u * (m + n))).ln()
                + ln_binomial_raw(0.5 * m, 0.5 * (m + n), a.u, a.v)
        }
    }
    pub fn cdf(&self, x: f64) -> f64 {
        self.tails(x).0
    }
    pub fn sf(&self, x: f64) -> f64 {
        self.tails(x).1
    }
    pub fn logcdf(&self, x: f64) -> f64 {
        if let Some((lower, _)) = Self::exact(x) {
            return lower.ln();
        }
        let (lower, upper) = self.tails(x);
        log_tail(lower, upper, || self.ln_lower(&self.args(x)))
    }
    pub fn logsf(&self, x: f64) -> f64 {
        if let Some((_, upper)) = Self::exact(x) {
            return upper.ln();
        }
        let (lower, upper) = self.tails(x);
        log_tail(upper, lower, || self.ln_upper(&self.args(x)))
    }
    pub fn quantile(&self, p: f64) -> f64 {
        let tails = LogTails {
            log_cdf: &|x| self.logcdf(x),
            log_sf: &|x| self.logsf(x),
            log_pdf: &|x| self.logpdf(x),
        };
        quantile::continuous(p, 1.0, 0.0, f64::INFINITY, &tails)
    }
}
