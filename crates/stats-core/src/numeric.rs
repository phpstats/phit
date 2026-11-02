//! Written from the clean-room specification specs/cleanroom/smoothing.md, by an implementer who had not seen R's source
//! or phit's earlier code; sources: Wand (1994), Hyndman and Fan (1996), and R's documentation and observed output. See THIRD_PARTY_NOTICES.md.

use crate::smoothing::Method;

pub fn linear_binning(x: &[f64], w: &[f64], lo: f64, hi: f64, n: usize) -> Vec<f64> {
    let mut bins = vec![0.0; 2 * n];
    let step = (hi - lo) / (n as f64 - 1.0);
    let last = n as f64 - 1.0;
    for (&xi, &wi) in x.iter().zip(w.iter()) {
        if !xi.is_finite() {
            continue;
        }
        let t = (xi - lo) / step;
        let left = t.floor();
        // Both target bins lie outside 0..n-1: nothing to add (also guards the integer cast).
        if !(left >= -1.0 && left <= last) {
            continue;
        }
        let frac = t - left;
        let k = left as i64;
        if k >= 0 {
            bins[k as usize] += wi * (1.0 - frac);
        }
        let k1 = k + 1;
        if k1 >= 0 && (k1 as usize) < n {
            bins[k1 as usize] += wi * frac;
        }
    }
    bins
}

/// Interpolation at v from points (x[i], y[i]) with x strictly increasing (at least 2 points). Values left of x[0]
/// are `low`; values right of the last x are `high`.
pub fn interpolate(x: &[f64], y: &[f64], v: f64, method: &Method, low: f64, high: f64) -> f64 {
    if v.is_nan() {
        return f64::NAN;
    }
    let m = x.len();
    if v < x[0] {
        return low;
    }
    if v > x[m - 1] {
        return high;
    }
    // First index whose x is >= v.
    let pos = x.partition_point(|&xi| xi < v);
    if x[pos] == v {
        return y[pos];
    }
    let (a, b) = (pos - 1, pos);
    match *method {
        // The spec formula y0 + (y1 - y0) * t, with the multiply-add fused (one rounding); this reproduces R output
        // bit for bit on the approx table, where the unfused form differs by an ulp in a few places.
        Method::Linear => (y[b] - y[a]).mul_add((v - x[a]) / (x[b] - x[a]), y[a]),
        Method::Constant(f) => {
            let left_weight = 1.0 - f;
            match (left_weight != 0.0, f != 0.0) {
                (true, true) => y[a] * left_weight + y[b] * f,
                (true, false) => y[a] * left_weight,
                (false, true) => y[b] * f,
                (false, false) => 0.0,
            }
        }
    }
}

/// 1 / sqrt(2π) rounded to double.
const RECIP_SQRT_TAU: f64 = 0.398_942_280_401_432_7;

/// The standard normal density φ(z) = exp(-z²/2) / sqrt(2π).
pub fn standard_normal_density(z: f64) -> f64 {
    if z.is_nan() {
        return f64::NAN;
    }
    let a = z.abs();
    // Beyond 40 the true value (below 1e-347) rounds to zero; this also covers infinities.
    if a > 40.0 {
        return 0.0;
    }
    // Split a = head + tail with head on a 2^-16 grid, so head has at most 22 significant bits and head² is exact.
    let scale = 65536.0;
    let head = (a * scale).trunc() / scale;
    let tail = a - head; // exact
    let half_head_sq = 0.5 * (head * head); // exact
    let rest = head * tail + 0.5 * (tail * tail); // below 6.2e-4, so its rounding error is negligible
    let small_factor = (-rest).exp();
    if half_head_sq < 700.0 {
        return (-half_head_sq).exp() * small_factor * RECIP_SQRT_TAU;
    }
    // Deep tail: keep every intermediate in the normal range, then scale down by an exact power of two so the only
    // rounding into the subnormal range happens once, at the end.
    let shift: f64 = 600.0; // half_head_sq - shift is exact
    let base = (-shift).exp() * 2f64.powi(800);
    (base * (-(half_head_sq - shift)).exp() * small_factor * RECIP_SQRT_TAU) * 2f64.powi(-800)
}

/// The type-7 sample quantile of sorted, finite data (at least one value) at probability p in [0, 1].
pub fn quantile_type7(sorted: &[f64], p: f64) -> f64 {
    let n = sorted.len();
    let h = (n as f64 - 1.0) * p + 1.0;
    let j = h.floor();
    let gamma = h - j;
    let j = (j as usize).clamp(1, n);
    let xj = sorted[j - 1];
    if gamma == 0.0 || j >= n {
        return xj;
    }
    let xj1 = sorted[j];
    if xj1 == xj {
        return xj;
    }
    (1.0 - gamma) * xj + gamma * xj1
}
