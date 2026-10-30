//! Kernel density estimation, interpolation and the empirical CDF, following
//! R's stats::density, stats::approx and stats::ecdf.
//!
//! [`density`] reproduces R's algorithm step for step: linear binning onto a
//! grid of n points (R's C BinDist), circular convolution with the kernel by
//! FFT, then linear interpolation onto the output grid. Grids use R's seq.int
//! fill, so grid points agree exactly. Inputs must be finite; dropping or
//! rejecting missing values is the caller's policy.
use crate::descriptive;
use crate::{ErrorKind, StatsError};
use std::f64::consts::PI;

fn error(kind: ErrorKind, message: &str) -> StatsError {
    StatsError::new(kind, message)
}

fn invalid(message: &str) -> StatsError {
    error(ErrorKind::InvalidInput, message)
}

fn check_finite(values: &[f64], what: &str) -> Result<(), StatsError> {
    if values.iter().any(|v| !v.is_finite()) {
        return Err(error(
            ErrorKind::Nonfinite,
            &format!("{what} must be finite"),
        ));
    }
    Ok(())
}

/// R's seq.int(from, to, length.out = n): exact ends, the first half stepped
/// from `from` and the second half from `to`.
pub fn seq(from: f64, to: f64, n: usize) -> Vec<f64> {
    let mut out = vec![0.0; n];
    if n > 0 {
        out[0] = from;
    }
    if n > 1 {
        out[n - 1] = to;
    }
    if n > 2 {
        let by = (to - from) / (n - 1) as f64;
        for (i, v) in out.iter_mut().enumerate().take(n - 1).skip(1) {
            *v = if i < n / 2 {
                from + i as f64 * by
            } else {
                to - (n - 1 - i) as f64 * by
            };
        }
    }
    out
}

fn check_bandwidth_input(x: &[f64]) -> Result<(), StatsError> {
    check_finite(x, "values")?;
    if x.len() < 2 {
        return Err(invalid("bandwidth selection requires at least two values"));
    }
    Ok(())
}

/// R's bw.nrd0: Silverman's rule of thumb,
/// 0.9 * min(sd, IQR / 1.34) * n^(-1/5), with R's fallbacks when that is zero.
pub fn bw_nrd0(x: &[f64]) -> Result<f64, StatsError> {
    check_bandwidth_input(x)?;
    let hi = descriptive::sd(x, false)?;
    let q = descriptive::quantiles(x, &[0.25, 0.75])?;
    let mut lo = hi.min((q[1] - q[0]) / 1.34);
    if lo == 0.0 {
        lo = if hi != 0.0 {
            hi
        } else if x[0] != 0.0 {
            x[0].abs()
        } else {
            1.0
        };
    }
    Ok(0.9 * lo * (x.len() as f64).powf(-0.2))
}

/// R's bw.nrd: Scott's variation, 1.06 * min(sd, IQR / 1.34) * n^(-1/5).
pub fn bw_nrd(x: &[f64]) -> Result<f64, StatsError> {
    check_bandwidth_input(x)?;
    let q = descriptive::quantiles(x, &[0.25, 0.75])?;
    let h = (q[1] - q[0]) / 1.34;
    Ok(1.06 * descriptive::sd(x, false)?.min(h) * (x.len() as f64).powf(-1.0 / 5.0))
}

/// Smoothing kernels of R's density(), each scaled to standard deviation bw.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kernel {
    Gaussian,
    Epanechnikov,
    Rectangular,
    Triangular,
    Biweight,
    Cosine,
    Optcosine,
}

impl Kernel {
    /// The kernel for one of R's names, or None.
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "gaussian" => Self::Gaussian,
            "epanechnikov" => Self::Epanechnikov,
            "rectangular" => Self::Rectangular,
            "triangular" => Self::Triangular,
            "biweight" => Self::Biweight,
            "cosine" => Self::Cosine,
            "optcosine" => Self::Optcosine,
            _ => return None,
        })
    }

    fn eval(self, x: f64, bw: f64) -> f64 {
        let ax = x.abs();
        let inside = |a: f64, value: f64| if ax < a { value } else { 0.0 };
        match self {
            Self::Gaussian => dnorm(x, bw),
            Self::Rectangular => {
                let a = bw * 3f64.sqrt();
                inside(a, 0.5 / a)
            }
            Self::Triangular => {
                let a = bw * 6f64.sqrt();
                inside(a, (1.0 - ax / a) / a)
            }
            Self::Epanechnikov => {
                let a = bw * 5f64.sqrt();
                inside(a, 3.0 / 4.0 * (1.0 - (ax / a).powi(2)) / a)
            }
            Self::Biweight => {
                let a = bw * 7f64.sqrt();
                inside(a, 15.0 / 16.0 * (1.0 - (ax / a).powi(2)).powi(2) / a)
            }
            Self::Cosine => {
                let a = bw / (1.0 / 3.0 - 2.0 / (PI * PI)).sqrt();
                inside(a, (1.0 + (PI * x / a).cos()) / (2.0 * a))
            }
            Self::Optcosine => {
                let a = bw / (1.0 - 8.0 / (PI * PI)).sqrt();
                inside(a, PI / 4.0 * (PI * x / (2.0 * a)).cos() / a)
            }
        }
    }
}

const FRAC_1_SQRT_2PI: f64 = 0.398_942_280_401_432_677_939_946_059_934;

/// R's dnorm(x, 0, sigma), including its split evaluation of exp(-x²/2) in the
/// tail, so kernel heights agree with R to the last bit.
fn dnorm(x: f64, sigma: f64) -> f64 {
    let x = (x / sigma).abs();
    if !x.is_finite() || x >= 2.0 * f64::MAX.sqrt() {
        return 0.0;
    }
    if x < 5.0 {
        return FRAC_1_SQRT_2PI * (-0.5 * x * x).exp() / sigma;
    }
    // sqrt(-2 ln 2 (DBL_MIN_EXP + 1 - DBL_MANT_DIG))
    let underflow =
        (-2.0 * std::f64::consts::LN_2 * (f64::MIN_EXP + 1 - f64::MANTISSA_DIGITS as i32) as f64)
            .sqrt();
    if x > underflow {
        return 0.0;
    }
    let x1 = (x * 65536.0).round_ties_even() / 65536.0;
    let x2 = x - x1;
    FRAC_1_SQRT_2PI / sigma * ((-0.5 * x1 * x1).exp() * ((-0.5 * x2 - x1) * x2).exp())
}

/// The bandwidth argument of [`density`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Bandwidth {
    Nrd0,
    Nrd,
    Fixed(f64),
}

/// Options of [`density`], with R's defaults.
#[derive(Debug, Clone)]
pub struct DensityOptions {
    pub bw: Bandwidth,
    pub adjust: f64,
    pub kernel: Kernel,
    pub n: usize,
    pub from: Option<f64>,
    pub to: Option<f64>,
    pub cut: f64,
    /// Observation weights; None means 1/n each. Not normalized, as in R.
    pub weights: Option<Vec<f64>>,
}

impl Default for DensityOptions {
    fn default() -> Self {
        Self {
            bw: Bandwidth::Nrd0,
            adjust: 1.0,
            kernel: Kernel::Gaussian,
            n: 512,
            from: None,
            to: None,
            cut: 3.0,
            weights: None,
        }
    }
}

/// A density estimate on an evenly spaced grid.
#[derive(Debug, Clone)]
pub struct Density {
    pub x: Vec<f64>,
    pub y: Vec<f64>,
    /// The bandwidth used, after `adjust`.
    pub bw: f64,
    /// The number of observations.
    pub n: usize,
}

/// R's C BinDist: linear binning of weighted points onto n grid points from
/// lo to hi, returned zero-padded to length 2n.
fn bin_dist(x: &[f64], w: &[f64], lo: f64, hi: f64, n: usize) -> Vec<f64> {
    let mut y = vec![0.0; 2 * n];
    let ixmax = n as i64 - 2;
    let delta = (hi - lo) / (n - 1) as f64;
    for (&xi, &wi) in x.iter().zip(w) {
        let pos = (xi - lo) / delta;
        let ix = pos.floor() as i64;
        let fx = pos - ix as f64;
        if (0..=ixmax).contains(&ix) {
            let i = ix as usize;
            y[i] += wi * (1.0 - fx);
            y[i + 1] += wi * fx;
        } else if ix == -1 {
            y[0] += wi * fx;
        } else if ix == ixmax + 1 {
            y[ix as usize] += wi * (1.0 - fx);
        }
    }
    y
}

/// In-place iterative radix-2 FFT; `inverse` uses the positive exponent and
/// does not scale, like R's fft(inverse = TRUE).
fn fft(re: &mut [f64], im: &mut [f64], inverse: bool) {
    let n = re.len();
    debug_assert!(n.is_power_of_two());
    let mut j = 0;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let sign = if inverse { 1.0 } else { -1.0 };
    let mut len = 2;
    while len <= n {
        let half = len / 2;
        let step = sign * 2.0 * PI / len as f64;
        for k in 0..half {
            // Twiddles computed directly rather than by recurrence keep the
            // rounding error at a few ulps for large transforms.
            let (s, c) = (step * k as f64).sin_cos();
            for start in (0..n).step_by(len) {
                let (a, b) = (start + k, start + k + half);
                let tr = re[b] * c - im[b] * s;
                let ti = re[b] * s + im[b] * c;
                re[b] = re[a] - tr;
                im[b] = im[a] - ti;
                re[a] += tr;
                im[a] += ti;
            }
        }
        len <<= 1;
    }
}

/// R's density() for finite x.
pub fn density(x: &[f64], options: &DensityOptions) -> Result<Density, StatsError> {
    check_finite(x, "values")?;
    if x.is_empty() {
        return Err(invalid("density requires at least one value"));
    }
    let nx = x.len();
    let weights = match &options.weights {
        None => vec![1.0 / nx as f64; nx],
        Some(w) => {
            if w.len() != nx {
                return Err(error(
                    ErrorKind::DimensionMismatch,
                    "values and weights must have the same length",
                ));
            }
            check_finite(w, "weights")?;
            if w.iter().any(|&w| w < 0.0) {
                return Err(invalid("weights must be nonnegative"));
            }
            w.clone()
        }
    };
    if options.n == 0 {
        return Err(invalid("n must be at least 1"));
    }
    let n_user = options.n;
    let n = if n_user <= 512 {
        512
    } else {
        n_user.next_power_of_two()
    };
    let bw = match options.bw {
        Bandwidth::Nrd0 => bw_nrd0(x)?,
        Bandwidth::Nrd => bw_nrd(x)?,
        Bandwidth::Fixed(bw) => bw,
    };
    if !bw.is_finite() || !options.adjust.is_finite() || !options.cut.is_finite() {
        return Err(error(
            ErrorKind::Nonfinite,
            "bw, adjust and cut must be finite",
        ));
    }
    let bw = options.adjust * bw;
    if bw <= 0.0 {
        return Err(invalid("the bandwidth must be positive"));
    }
    let (min, max) = x
        .iter()
        .fold((f64::INFINITY, f64::NEG_INFINITY), |(a, b), &v| {
            (a.min(v), b.max(v))
        });
    let from = options.from.unwrap_or(min - options.cut * bw);
    let to = options.to.unwrap_or(max + options.cut * bw);
    if !from.is_finite() || !to.is_finite() {
        return Err(error(ErrorKind::Nonfinite, "from and to must be finite"));
    }
    if from >= to {
        return Err(invalid("from must be less than to"));
    }
    let (lo, up) = (from - 4.0 * bw, to + 4.0 * bw);

    let mut y_re = bin_dist(x, &weights, lo, up, n);
    let mut kords = seq(0.0, (2 * n - 1) as f64 / (n - 1) as f64 * (up - lo), 2 * n);
    for j in n + 1..2 * n {
        kords[j] = -kords[2 * n - j];
    }
    let mut k_re: Vec<f64> = kords.iter().map(|&k| options.kernel.eval(k, bw)).collect();
    let (mut y_im, mut k_im) = (vec![0.0; 2 * n], vec![0.0; 2 * n]);
    fft(&mut y_re, &mut y_im, false);
    fft(&mut k_re, &mut k_im, false);
    // fft(y) * Conj(fft(kords))
    for i in 0..2 * n {
        let (a, b, c, d) = (y_re[i], y_im[i], k_re[i], -k_im[i]);
        y_re[i] = a * c - b * d;
        y_im[i] = a * d + b * c;
    }
    fft(&mut y_re, &mut y_im, true);
    let scale = (2 * n) as f64;
    let heights: Vec<f64> = y_re[..n].iter().map(|&v| (v / scale).max(0.0)).collect();
    let grid = seq(lo, up, n);
    let out_x = seq(from, to, n_user);
    let out_y = out_x
        .iter()
        .map(|&v| interpolate(&grid, &heights, v, Method::Linear, f64::NAN, f64::NAN))
        .collect();
    Ok(Density {
        x: out_x,
        y: out_y,
        bw,
        n: nx,
    })
}

/// Interpolation method of [`approx`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Method {
    Linear,
    /// Step function; f in [0, 1] weights the right-hand value (R's f).
    Constant(f64),
}

/// How [`approx`] collapses tied x values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ties {
    Mean,
    Min,
    Max,
}

/// What [`approx`] returns outside [min(x), max(x)].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// NaN, R's rule = 1.
    Missing,
    /// The value at the closest data extreme, R's rule = 2.
    Nearest,
}

// R's C approx1, for sorted unique x.
fn interpolate(x: &[f64], y: &[f64], v: f64, method: Method, low: f64, high: f64) -> f64 {
    if v.is_nan() {
        return f64::NAN;
    }
    let (mut i, mut j) = (0, x.len() - 1);
    if v < x[i] {
        return low;
    }
    if v > x[j] {
        return high;
    }
    while i + 1 < j {
        let ij = (i + j) / 2;
        if v < x[ij] {
            j = ij;
        } else {
            i = ij;
        }
    }
    if v == x[j] {
        return y[j];
    }
    if v == x[i] {
        return y[i];
    }
    match method {
        Method::Linear => y[i] + (y[j] - y[i]) * ((v - x[i]) / (x[j] - x[i])),
        Method::Constant(f) => {
            let (f1, f2) = (1.0 - f, f);
            (if f1 != 0.0 { y[i] * f1 } else { 0.0 }) + (if f2 != 0.0 { y[j] * f2 } else { 0.0 })
        }
    }
}

/// R's approx(x, y, xout): sorts by x, collapses ties, then interpolates.
/// NaN in xout gives NaN; infinite xout is outside the data.
pub fn approx(
    x: &[f64],
    y: &[f64],
    xout: &[f64],
    method: Method,
    rule: Rule,
    ties: Ties,
) -> Result<Vec<f64>, StatsError> {
    if x.len() != y.len() {
        return Err(error(
            ErrorKind::DimensionMismatch,
            "x and y must have the same length",
        ));
    }
    check_finite(x, "x")?;
    check_finite(y, "y")?;
    if let Method::Constant(f) = method
        && !(0.0..=1.0).contains(&f)
    {
        return Err(invalid("f must be between 0 and 1"));
    }
    let mut order: Vec<usize> = (0..x.len()).collect();
    order.sort_by(|&a, &b| x[a].total_cmp(&x[b]));
    let (mut ux, mut uy) = (Vec::new(), Vec::new());
    let mut start = 0;
    while start < order.len() {
        let mut end = start + 1;
        while end < order.len() && x[order[end]] == x[order[start]] {
            end += 1;
        }
        let group: Vec<f64> = order[start..end].iter().map(|&i| y[i]).collect();
        ux.push(x[order[start]]);
        uy.push(match ties {
            Ties::Mean => descriptive::mean(&group)?,
            Ties::Min => group.iter().copied().fold(f64::INFINITY, f64::min),
            Ties::Max => group.iter().copied().fold(f64::NEG_INFINITY, f64::max),
        });
        start = end;
    }
    match (ux.len(), method) {
        (0, _) => return Err(invalid("approx requires at least one value")),
        (1, Method::Linear) => {
            return Err(invalid(
                "linear interpolation requires at least two distinct x values",
            ));
        }
        _ => {}
    }
    let (low, high) = match rule {
        Rule::Missing => (f64::NAN, f64::NAN),
        Rule::Nearest => (uy[0], uy[uy.len() - 1]),
    };
    Ok(xout
        .iter()
        .map(|&v| interpolate(&ux, &uy, v, method, low, high))
        .collect())
}

/// The empirical CDF of x at each q: the proportion of x at or below q.
/// NaN in q gives NaN.
pub fn ecdf(x: &[f64], q: &[f64]) -> Result<Vec<f64>, StatsError> {
    check_finite(x, "values")?;
    if x.is_empty() {
        return Err(invalid("ecdf requires at least one value"));
    }
    let mut sorted = x.to_vec();
    sorted.sort_unstable_by(f64::total_cmp);
    let n = sorted.len() as f64;
    Ok(q.iter()
        .map(|&v| {
            if v.is_nan() {
                f64::NAN
            } else {
                sorted.partition_point(|&s| s <= v) as f64 / n
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seq_matches_r_fill() {
        assert_eq!(seq(0.0, 1.0, 5), [0.0, 0.25, 0.5, 0.75, 1.0]);
        assert_eq!(seq(3.0, 4.0, 1), [3.0]);
        assert!(seq(0.0, 1.0, 0).is_empty());
    }

    #[test]
    fn fft_round_trip() {
        let mut re = vec![1.0, 2.0, 3.0, 4.0, 0.0, -1.0, 0.5, 2.5];
        let mut im = vec![0.0; 8];
        let original = re.clone();
        fft(&mut re, &mut im, false);
        assert!((re[0] - 12.0).abs() < 1e-12);
        fft(&mut re, &mut im, true);
        for (a, b) in re.iter().zip(&original) {
            assert!((a / 8.0 - b).abs() < 1e-14);
        }
    }

    #[test]
    fn density_integrates_to_one() {
        let x = [1.0, 2.0, 2.5, 4.0, 7.0];
        let d = density(&x, &DensityOptions::default()).unwrap();
        let dx = d.x[1] - d.x[0];
        let area: f64 = d.y.iter().sum::<f64>() * dx;
        assert!((area - 1.0).abs() < 1e-3, "{area}");
        assert_eq!(d.n, 5);
    }

    #[test]
    fn approx_rules_and_ties() {
        let out = approx(
            &[1.0, 2.0, 2.0, 3.0],
            &[10.0, 20.0, 40.0, 30.0],
            &[0.0, 1.5, 2.0, 4.0, f64::NAN],
            Method::Linear,
            Rule::Missing,
            Ties::Mean,
        )
        .unwrap();
        assert!(out[0].is_nan() && out[4].is_nan() && out[3].is_nan());
        assert_eq!(&out[1..3], &[20.0, 30.0]);
        let out = approx(
            &[1.0, 2.0],
            &[5.0, 7.0],
            &[0.0, 1.5, 9.0],
            Method::Constant(0.0),
            Rule::Nearest,
            Ties::Mean,
        )
        .unwrap();
        assert_eq!(out, [5.0, 5.0, 7.0]);
    }

    #[test]
    fn ecdf_counts() {
        let out = ecdf(&[3.0, 1.0, 2.0, 2.0], &[0.0, 2.0, 2.5, 3.0]).unwrap();
        assert_eq!(out, [0.0, 0.75, 0.75, 1.0]);
    }

    #[test]
    fn rejections() {
        assert!(bw_nrd0(&[1.0]).is_err());
        assert!(density(&[], &DensityOptions::default()).is_err());
        assert!(
            approx(
                &[1.0],
                &[1.0],
                &[1.0],
                Method::Linear,
                Rule::Missing,
                Ties::Mean
            )
            .is_err()
        );
    }
}
