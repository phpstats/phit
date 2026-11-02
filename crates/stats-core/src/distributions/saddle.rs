//! Written from the clean-room specification specs/cleanroom/special.md, by an implementer who had not seen R's source
//! or phit's earlier code; sources: Loader (2000), Abramowitz and Stegun, Lentz (1976), and Thompson and Barnett (1986). See THIRD_PARTY_NOTICES.md.

use statrs::function::gamma::ln_gamma;

const LN_2PI: f64 = 1.837_877_066_409_345_5; // ln(2π)
const EPS: f64 = f64::EPSILON;
/// Iteration cap for the series and continued fractions.
const MAX_ITER: usize = 1_000_000;

/// δ(k/2) for k = 1..=30 (n = 0.5, 1.0, ..., 15.0).
///
/// Generated with mpmath at 60 significant digits:
/// ```text
/// mp.mp.dps = 60
/// for k in range(1, 31):
///     n = mp.mpf(k) / 2
///     d = mp.loggamma(n + 1) - (n + mp.mpf(1) / 2) * mp.log(n) + n - mp.log(mp.sqrt(2 * mp.pi))
///     print(repr(float(d)))
/// ```
const DELTA_HALF: [f64; 30] = [
    0.15342640972002736,   // 0.5
    0.08106146679532726,   // 1.0
    0.05481412105191765,   // 1.5
    0.0413406959554093,    // 2.0
    0.03316287351993629,   // 2.5
    0.02767792568499834,   // 3.0
    0.023746163656297496,  // 3.5
    0.020790672103765093,  // 4.0
    0.018488450532673187,  // 4.5
    0.016644691189821193,  // 5.0
    0.015134973221917378,  // 5.5
    0.013876128823070748,  // 6.0
    0.012810465242920227,  // 6.5
    0.01189670994589177,   // 7.0
    0.011104559758206917,  // 7.5
    0.010411265261972096,  // 8.0
    0.009799416126158804,  // 8.5
    0.009255462182712733,  // 9.0
    0.008768700134139386,  // 9.5
    0.00833056343336287,   // 10.0
    0.00793411456431402,   // 10.5
    0.007573675487951841,  // 11.0
    0.007244554301320383,  // 11.5
    0.00694284010720953,   // 12.0
    0.006665247032707682,  // 12.5
    0.006408994188004207,  // 13.0
    0.006171712263039458,  // 13.5
    0.0059513701127588475, // 14.0
    0.0057462165130101155, // 14.5
    0.005554733551962801,  // 15.0
];

/// Coefficients B_{2k} / (2k (2k - 1)) of the asymptotic series δ(n) ~ Σ c_k n^{1-2k}, k = 1..=9.
const STIRLING_COEFS: [f64; 9] = [
    1.0 / 12.0,
    -1.0 / 360.0,
    1.0 / 1260.0,
    -1.0 / 1680.0,
    1.0 / 1188.0,
    -691.0 / 360_360.0,
    1.0 / 156.0,
    -3617.0 / 122_400.0,
    43_867.0 / 244_188.0,
];

/// Asymptotic series for δ(n), n >= 15. At n = 15 the first omitted term (k = 10) is below 1e-21, far under the
/// rounding of δ(15) ≈ 5.6e-3.
fn stirling_asymptotic(n: f64) -> f64 {
    let r = 1.0 / n;
    let r2 = r * r;
    let mut acc = 0.0;
    for &c in STIRLING_COEFS.iter().rev() {
        acc = acc * r2 + c;
    }
    acc * r
}

/// δ(n) - δ(n + 1) = (n + 1/2) ln(1 + 1/n) - 1 = atanh(t)/t - 1 with t = 1/(2n + 1),
/// = t²/3 + t⁴/5 + t⁶/7 + ..., a sum of positive terms with no cancellation.
fn stirling_step(n: f64) -> f64 {
    let t = 1.0 / (2.0 * n + 1.0);
    if t > 0.5 {
        // Only for n < 1/2: atanh(t)/t >= 1.098, so the subtraction loses at most one digit.
        let atanh = 0.5 * (2.0 * t / (1.0 - t)).ln_1p();
        return atanh / t - 1.0;
    }
    let t2 = t * t;
    let mut pow = t2;
    let mut sum = 0.0;
    let mut k = 3.0;
    loop {
        let next = sum + pow / k;
        if next == sum {
            break;
        }
        sum = next;
        pow *= t2;
        k += 2.0;
    }
    sum
}

/// δ(n) = ln Γ(n + 1) - (n + 1/2) ln n + n - ln √(2π), for n > 0.
pub fn stirling_error(n: f64) -> f64 {
    if n.is_nan() || n < 0.0 {
        return f64::NAN;
    }
    if n == 0.0 {
        return f64::INFINITY;
    }
    if n >= 15.0 {
        return stirling_asymptotic(n);
    }
    let twice = 2.0 * n;
    if twice == twice.floor() {
        return DELTA_HALF[twice as usize - 1];
    }
    // Climb with the step relation until n + k >= 15, then use the asymptotic series. All added terms are positive.
    let mut m = n;
    let mut steps = 0.0;
    while m < 15.0 {
        steps += stirling_step(m);
        m += 1.0;
    }
    stirling_asymptotic(m) + steps
}

/// δ(t) + ½ ln t, the combination in which δ enters the saddle-point forms. For t < 1, δ(t) grows like -½ ln t and
/// the two terms cancel, so the sum is regrouped exactly as ln Γ(t + 1) - t ln t + t - ln √(2π), whose terms are all
/// small.
fn stirling_plus_half_log(t: f64) -> f64 {
    if t < 1.0 {
        ln_gamma(t + 1.0) - t * t.ln() + t - 0.5 * LN_2PI
    } else {
        stirling_error(t) + 0.5 * t.ln()
    }
}

/// Above this, x + np or 2x may overflow inside the deviance term.
const DEVIANCE_SCALE_LIMIT: f64 = f64::MAX / 4.0;

/// x ln(x / np) + np - x for x >= 0 and np > 0, accurate when x ≈ np (where both terms nearly cancel).
pub fn deviance_term(x: f64, np: f64) -> f64 {
    // D is homogeneous of degree one, D(cx, c·np) = c·D(x, np), and scaling by a power of two is exact. Near the top
    // of the range, evaluate at a quarter of the scale, where x + np and 2x stay finite.
    if x > DEVIANCE_SCALE_LIMIT || np > DEVIANCE_SCALE_LIMIT {
        return 4.0 * deviance_unscaled(0.25 * x, 0.25 * np);
    }
    let d = deviance_unscaled(x, np);
    if d == f64::INFINITY {
        // x ln(x / np) overflowed although D may be finite; at a quarter scale only a D above the range overflows.
        return 4.0 * deviance_unscaled(0.25 * x, 0.25 * np);
    }
    d
}

/// The deviance term where x + np and 2x are finite. A tiny x scaled to zero also gives np.
fn deviance_unscaled(x: f64, np: f64) -> f64 {
    if x == 0.0 {
        return np;
    }
    let diff = x - np;
    let total = x + np;
    if diff.abs() < 0.1 * total {
        // With v = (x - np)/(x + np): D = (x - np) v + 2x (v³/3 + v⁵/5 + ...).
        let v = diff / total;
        let v2 = v * v;
        let mut pow = 2.0 * x * v; // 2x v^(2j+1), starting at j = 0
        let mut sum = diff * v;
        let mut k = 3.0;
        loop {
            pow *= v2;
            let next = sum + pow / k;
            // A non-finite sum cannot settle, so it ends the series rather than looping.
            if next == sum || !next.is_finite() {
                return next;
            }
            sum = next;
            k += 2.0;
        }
    }
    let ratio = x / np;
    let log_ratio = if ratio.is_finite() && ratio > 0.0 {
        ratio.ln()
    } else {
        x.ln() - np.ln()
    };
    x * log_ratio + np - x
}

/// ln(λ^x e^-λ / Γ(x + 1)) for real x >= 0 and λ >= 0, by the saddle-point form.
pub fn ln_poisson_raw(x: f64, lambda: f64) -> f64 {
    if lambda == 0.0 {
        return if x == 0.0 { 0.0 } else { f64::NEG_INFINITY };
    }
    if x == 0.0 {
        return -lambda;
    }
    // -δ(x) - D(x, λ) - ½ ln(2πx), with δ(x) grouped with its ½ ln x.
    -stirling_plus_half_log(x) - deviance_term(x, lambda) - 0.5 * LN_2PI
}

/// n ln q, computed from whichever of p and q = 1 - p is the more accurate argument.
fn n_ln_complement(n: f64, p: f64, q: f64) -> f64 {
    if p < 0.5 {
        n * (-p).ln_1p()
    } else {
        n * q.ln()
    }
}

/// Shared body: `x` successes and `m` failures out of `n` (n = x + m, supplied consistently by the caller).
fn ln_binomial_core(x: f64, m: f64, n: f64, p: f64, q: f64) -> f64 {
    if p == 0.0 {
        return if x == 0.0 { 0.0 } else { f64::NEG_INFINITY };
    }
    if q == 0.0 {
        return if m == 0.0 { 0.0 } else { f64::NEG_INFINITY };
    }
    if x == 0.0 {
        return n_ln_complement(n, p, q);
    }
    if m == 0.0 {
        return n_ln_complement(n, q, p);
    }
    // δ(n) - δ(x) - δ(m) - D(x, np) - D(m, nq) - ½ ln(2π x m / n), with each δ(t) grouped with its ½ ln t.
    let stirling =
        stirling_plus_half_log(n) - stirling_plus_half_log(x) - stirling_plus_half_log(m);
    let deviance = deviance_term(x, n * p) + deviance_term(m, n * q);
    stirling - deviance - 0.5 * LN_2PI
}

/// ln(Γ(n + 1) / (Γ(x + 1) Γ(n - x + 1)) p^x q^(n-x)) for real 0 <= x <= n, with q = 1 - p given separately and
/// accurately by the caller.
pub fn ln_binomial_raw(x: f64, n: f64, p: f64, q: f64) -> f64 {
    ln_binomial_core(x, n - x, n, p, q)
}

/// As ln_binomial_raw with n = x + m, for callers that know m = n - x exactly.
pub fn ln_binomial_raw_split(x: f64, m: f64, p: f64, q: f64) -> f64 {
    ln_binomial_core(x, m, x + m, p, q)
}

/// ln(x^a e^-x / Γ(a + 1)). For a >= 1 by the Poisson saddle-point form (no cancellation when a ≈ x is large);
/// for a < 1 the direct sum has no large terms to cancel.
fn ln_gamma_prefactor(a: f64, x: f64) -> f64 {
    if a >= 1.0 {
        ln_poisson_raw(a, x)
    } else {
        a * x.ln() - x - ln_gamma(a + 1.0)
    }
}

/// ln P(a, x), the regularized lower incomplete gamma function, by its power series; used for 0 < x < a + 1.
///
/// P(a, x) = x^a e^-x / Γ(a + 1) · Σ_{k>=0} x^k / ((a + 1)(a + 2)···(a + k))   (A&S 6.5.29).
pub fn ln_gamma_lower_series(a: f64, x: f64) -> f64 {
    if x == 0.0 {
        return f64::NEG_INFINITY;
    }
    let mut term = 1.0;
    let mut sum = 1.0;
    let mut comp = 0.0; // compensated (Kahan) summation
    let mut denom = a;
    let mut converged = false;
    for _ in 0..MAX_ITER {
        denom += 1.0;
        term *= x / denom;
        let y = term - comp;
        let t = sum + y;
        comp = (t - sum) - y;
        sum = t;
        if term <= sum * EPS * 0.5 {
            converged = true;
            break;
        }
    }
    if !converged {
        return f64::NAN;
    }
    ln_gamma_prefactor(a, x) + sum.ln()
}

/// Evaluates b0 + a1/(b1 + a2/(b2 + ...)) with the modified Lentz method. `terms(j)` returns (a_j, b_j) for j >= 1.
/// Returns NaN if the iteration cap is reached.
fn lentz<F: FnMut(usize) -> (f64, f64)>(b0: f64, mut terms: F) -> f64 {
    const TINY: f64 = 1e-300;
    let mut f = if b0 == 0.0 { TINY } else { b0 };
    let mut c = f;
    let mut d = 0.0;
    for j in 1..=MAX_ITER {
        let (aj, bj) = terms(j);
        d = bj + aj * d;
        if d == 0.0 {
            d = TINY;
        }
        c = bj + aj / c;
        if c == 0.0 {
            c = TINY;
        }
        d = 1.0 / d;
        let delta = c * d;
        f *= delta;
        if (delta - 1.0).abs() <= EPS {
            return f;
        }
    }
    f64::NAN
}

/// ln Q(a, x), the regularized upper incomplete gamma function, by its continued fraction evaluated with the
/// modified Lentz method; used for x > a + 1.
///
/// Contracting the continued fraction of A&S 6.5.31 gives
/// Γ(a, x) = e^-x x^a / (x + 1 - a - 1(1 - a)/(x + 3 - a - 2(2 - a)/(x + 5 - a - ...))),
/// and Q = Γ(a, x)/Γ(a), whose prefactor e^-x x^a / Γ(a) is a · x^a e^-x / Γ(a + 1).
pub fn ln_gamma_upper_fraction(a: f64, x: f64) -> f64 {
    let b1 = x + 1.0 - a;
    let denom = lentz(b1, |j| {
        let k = j as f64;
        (-k * (k - a), b1 + 2.0 * k)
    });
    if denom.is_nan() {
        return f64::NAN;
    }
    a.ln() + ln_gamma_prefactor(a, x) - denom.ln()
}

/// ln I_x(a, b), the regularized incomplete beta function, by its continued fraction (modified Lentz); used for
/// 0 < x < (a + 1)/(a + b + 2). The caller passes ln x and ln(1 - x), computed without cancellation.
///
/// I_x(a, b) = x^a (1 - x)^b / (a B(a, b)) · 1/(1 + d1/(1 + d2/(1 + ...)))   (A&S 26.5.8), with
/// d_{2m+1} = -(a + m)(a + b + m) x / ((a + 2m)(a + 2m + 1)) and d_{2m} = m (b - m) x / ((a + 2m - 1)(a + 2m)).
/// The prefactor equals b/(a + b) times the binomial probability of a successes and b failures with success
/// probability x, evaluated in Loader's saddle-point form.
pub fn ln_beta_lower_fraction(a: f64, b: f64, x: f64, ln_x: f64, ln_y: f64) -> f64 {
    let denom = lentz(1.0, |j| {
        let d = if j % 2 == 1 {
            let m = ((j - 1) / 2) as f64;
            -(a + m) * (a + b + m) * x / ((a + 2.0 * m) * (a + 2.0 * m + 1.0))
        } else {
            let m = (j / 2) as f64;
            m * (b - m) * x / ((a + 2.0 * m - 1.0) * (a + 2.0 * m))
        };
        (d, 1.0)
    });
    if denom.is_nan() {
        return f64::NAN;
    }
    let prefactor = if x < f64::MIN_POSITIVE {
        // x is 0 or subnormal, so it no longer carries the value that ln_x does, and the saddle-point form (which
        // needs (a + b)x) would see 0. The prefactor is then dominated by a ln x, so the direct log form
        // a ln x + b ln(1 - x) - ln(a B(a, b)) has nothing large to cancel.
        a * ln_x + b * ln_y - ln_a_beta(a, b)
    } else {
        let y = ln_y.exp();
        ln_binomial_raw_split(a, b, x, y) + (b / (a + b)).ln()
    };
    prefactor - denom.ln()
}

/// ln(a B(a, b)) = ln Γ(a + 1) + ln Γ(b) - ln Γ(a + b).
fn ln_a_beta(a: f64, b: f64) -> f64 {
    ln_gamma(a + 1.0) + ln_gamma(b) - ln_gamma(a + b)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows of one table in specs/cleanroom/data/special.json, every entry parsed from its decimal string.
    fn table(name: &str) -> Vec<Vec<f64>> {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../specs/cleanroom/data/special.json"
        );
        let text = std::fs::read_to_string(path).expect("special.json is readable");
        let json: serde_json::Value = serde_json::from_str(&text).expect("special.json parses");
        json[name]
            .as_array()
            .unwrap_or_else(|| panic!("table {name} exists"))
            .iter()
            .map(|row| {
                row.as_array()
                    .expect("row is an array")
                    .iter()
                    .map(|cell| match cell {
                        serde_json::Value::String(s) => match s.as_str() {
                            "Inf" | "inf" => f64::INFINITY,
                            "-Inf" | "-inf" => f64::NEG_INFINITY,
                            "NaN" | "nan" => f64::NAN,
                            s => s.parse().expect("cell is a number"),
                        },
                        other => other.as_f64().expect("cell is a number"),
                    })
                    .collect()
            })
            .collect()
    }

    /// Checks `f` on every row (inputs, then the expected value last) and returns the largest relative difference.
    /// `scaled` measures the error against max(1, |value|) rather than |value|.
    fn check(name: &str, tol: f64, scaled: bool, f: impl Fn(&[f64]) -> f64) -> f64 {
        let rows = table(name);
        assert!(!rows.is_empty(), "table {name} is empty");
        let mut worst = 0.0_f64;
        for row in &rows {
            let (inputs, expected) = row.split_at(row.len() - 1);
            let expected = expected[0];
            let got = f(inputs);
            if expected.is_infinite() || expected.is_nan() {
                assert!(
                    got == expected || (got.is_nan() && expected.is_nan()),
                    "{name} {inputs:?}: got {got}, want {expected}"
                );
                continue;
            }
            assert!(
                got.is_finite(),
                "{name} {inputs:?}: got {got}, want {expected}"
            );
            let scale = if scaled {
                expected.abs().max(1.0)
            } else {
                expected.abs()
            };
            let diff = if scale == 0.0 {
                (got - expected).abs()
            } else {
                (got - expected).abs() / scale
            };
            assert!(
                diff <= tol,
                "{name} {inputs:?}: got {got}, want {expected}, relative difference {diff:e}"
            );
            worst = worst.max(diff);
        }
        println!(
            "{name}: {} rows, largest relative difference {worst:e}",
            rows.len()
        );
        worst
    }

    #[test]
    fn stirling_error_table() {
        check("stirling_error", 1e-14, false, |r| stirling_error(r[0]));
    }

    #[test]
    fn deviance_term_table() {
        check("deviance_term", 1e-13, false, |r| deviance_term(r[0], r[1]));
    }

    fn relative_difference(got: f64, want: f64) -> f64 {
        (got - want).abs() / want.abs()
    }

    #[test]
    fn deviance_term_near_the_top_of_the_range() {
        // x + np and 2x overflow here; the series once took 0 · ∞ = NaN for its first term and never settled.
        // Expected values are x ln(x / np) + np - x at 60 digits (mpmath) for the exact doubles.
        let cases = [
            (1e308, 9.9e307, 5.033_585_350_144_054e303),
            (f64::MAX, 1.7e308, 2.754_767_021_251_925_8e305),
            // x ln(x / np) alone overflows, but D does not.
            (4.4e307, 6.6e305, 1.414_470_234_267_167_8e308),
            // A tiny x against a huge np, where a quarter of x underflows to zero.
            (5e-324, 1e308, 1e308),
            (1e-300, 1e308, 1e308),
        ];
        for (x, np, want) in cases {
            let got = deviance_term(x, np);
            assert!(
                relative_difference(got, want) <= 1e-13,
                "deviance_term({x:e}, {np:e}) = {got:e}, want {want:e}"
            );
        }
        assert_eq!(deviance_term(f64::MAX, f64::MAX), 0.0);
        // D above the largest double.
        assert_eq!(deviance_term(1e308, 1e-300), f64::INFINITY);
    }

    #[test]
    fn deviance_term_scales_exactly_by_powers_of_two() {
        // D(cx, c·np) = c·D(x, np), and multiplying by a power of two is exact, so the scaled evaluation must agree
        // bit for bit with the unscaled one on both sides of the scaling limit.
        for (x, np) in [(1.9, 1.8), (1.0, 1.05), (1.9, 0.2), (0.3, 1.9)] {
            let small = deviance_term(x, np);
            for k in [1000, 1021, 1022, 1023] {
                let c = 2f64.powi(k);
                assert_eq!(
                    deviance_term(x * c, np * c),
                    small * c,
                    "D({x} · 2^{k}, {np} · 2^{k})"
                );
            }
        }
    }

    #[test]
    fn ln_poisson_raw_table() {
        check("ln_poisson_raw", 1e-13, true, |r| {
            ln_poisson_raw(r[0], r[1])
        });
    }

    #[test]
    fn ln_binomial_raw_table() {
        check("ln_binomial_raw", 1e-13, true, |r| {
            ln_binomial_raw(r[0], r[1], r[2], r[3])
        });
    }

    #[test]
    fn ln_gamma_lower_table() {
        check("ln_gamma_lower", 1e-13, true, |r| {
            ln_gamma_lower_series(r[0], r[1])
        });
    }

    #[test]
    fn ln_gamma_upper_table() {
        check("ln_gamma_upper", 1e-13, true, |r| {
            ln_gamma_upper_fraction(r[0], r[1])
        });
    }

    #[test]
    fn ln_beta_lower_table() {
        check("ln_beta_lower", 1e-13, true, |r| {
            let (a, b, x) = (r[0], r[1], r[2]);
            ln_beta_lower_fraction(a, b, x, x.ln(), (-x).ln_1p())
        });
    }

    #[test]
    fn ln_beta_lower_tiny_table() {
        check("ln_beta_lower_tiny", 1e-13, true, |r| {
            ln_beta_lower_fraction(r[0], r[1], r[2], r[3], r[4])
        });
    }

    /// Rows of ln_beta_lower_tiny, spelled out: x has underflowed to 0 and only ln x carries it.
    #[test]
    fn ln_beta_lower_uses_ln_x_when_x_underflows() {
        let rows: [[f64; 6]; 3] = [
            [
                386.2246550996532,
                147.97521919042075,
                0.0,
                -1901.6975759113536,
                0.0,
                -734171.7981716662,
            ],
            [
                0.1812168037140112,
                0.24869755023829715,
                0.0,
                -1302.7443614166948,
                0.0,
                -236.5698181603671,
            ],
            [
                0.02111065292370408,
                0.03457498483308624,
                0.0,
                -1090.1705650532017,
                0.0,
                -23.489650404712197,
            ],
        ];
        for [a, b, x, ln_x, ln_y, want] in rows {
            let got = ln_beta_lower_fraction(a, b, x, ln_x, ln_y);
            assert!(
                (got - want).abs() <= 1e-13 * want.abs().max(1.0),
                "a={a} b={b}: got {got}, want {want}"
            );
        }
    }

    /// A subnormal x takes the same route as 0: the result follows ln x, not the rounded x.
    #[test]
    fn ln_beta_lower_subnormal_x_follows_ln_x() {
        let ln_x = -720.0_f64;
        let x = ln_x.exp();
        assert!(x > 0.0 && x < f64::MIN_POSITIVE);
        let (a, b) = (2.5, 3.0);
        let want = a * ln_x - ln_a_beta(a, b);
        let got = ln_beta_lower_fraction(a, b, x, ln_x, (-x).ln_1p());
        assert!(
            (got - want).abs() <= 1e-13 * want.abs(),
            "got {got}, want {want}"
        );
    }
}
