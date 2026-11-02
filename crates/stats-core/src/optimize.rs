//! Written from the clean-room specification specs/cleanroom/brent.md, by an implementer who had not seen R's source
//! or phit's earlier code; sources: Brent (1973) and netlib's public-domain fmin.f and zeroin.f. See THIRD_PARTY_NOTICES.md.

pub const DEFAULT_TOL: f64 = 0.000_122_070_312_5;

/// A minimizer of f on [lower, upper] by Brent's method (golden section with parabolic interpolation).
/// Returns the final x. An error from f stops the search and is returned at once.
pub fn fmin<E>(
    lower: f64,
    upper: f64,
    tol: f64,
    mut f: impl FnMut(f64) -> Result<f64, E>,
) -> Result<f64, E> {
    let golden = 0.5 * (3.0 - 5.0_f64.sqrt());
    let eps = f64::EPSILON.sqrt();
    let third = tol / 3.0;

    let (mut lo, mut hi) = (lower, upper);
    let mut x = lo + golden * (hi - lo);
    let (mut w, mut v) = (x, x);
    let mut step = 0.0_f64; // d
    let mut prev_step = 0.0_f64; // e
    let mut fx = f(x)?;
    let (mut fw, mut fv) = (fx, fx);

    loop {
        let mid = 0.5 * (lo + hi);
        let tol1 = eps * x.abs() + third;
        let tol2 = 2.0 * tol1;
        if (x - mid).abs() <= tol2 - 0.5 * (hi - lo) {
            return Ok(x);
        }

        let (mut p, mut q, mut r) = (0.0_f64, 0.0_f64, 0.0_f64);
        if prev_step.abs() > tol1 {
            r = (x - w) * (fx - fv);
            q = (x - v) * (fx - fw);
            p = (x - v) * q - (x - w) * r;
            q = 2.0 * (q - r);
            if q > 0.0 {
                p = -p;
            } else {
                q = -q;
            }
            r = prev_step;
            prev_step = step;
        }

        let parabolic_ok =
            !(p.abs() >= (0.5 * q * r).abs() || p <= q * (lo - x) || p >= q * (hi - x));
        if parabolic_ok {
            step = p / q;
            let u = x + step;
            if !(u - lo >= tol2 && hi - u >= tol2) {
                step = if x >= mid { -tol1 } else { tol1 };
            }
        } else {
            prev_step = if x >= mid { lo - x } else { hi - x };
            step = golden * prev_step;
        }

        let u = if step.abs() >= tol1 {
            x + step
        } else if step <= 0.0 {
            x - tol1
        } else {
            x + tol1
        };
        let fu = f(u)?;

        if fu <= fx {
            if u >= x {
                lo = x;
            } else {
                hi = x;
            }
            v = w;
            fv = fw;
            w = x;
            fw = fx;
            x = u;
            fx = fu;
        } else {
            if u >= x {
                hi = u;
            } else {
                lo = u;
            }
            if fu <= fw || w == x {
                v = w;
                fv = fw;
                w = u;
                fw = fu;
            } else if fu <= fv || v == x || v == w {
                v = u;
                fv = fu;
            }
        }
    }
}

pub struct Zero {
    pub root: f64,
    /// Evaluations of f made by the search before convergence, or None when it stopped without converging.
    pub iterations: Option<usize>,
    /// The width of the final bracket, |c - b| (0 for a root at an endpoint).
    pub estimated_precision: f64,
}

/// A root of f in [lower, upper] by Brent's method, given f(lower) and f(upper), which have opposite signs or one
/// of which is zero. An error from f stops the search and is returned at once.
pub fn zeroin<E>(
    lower: f64,
    upper: f64,
    f_lower: f64,
    f_upper: f64,
    tol: f64,
    maxiter: usize,
    mut f: impl FnMut(f64) -> Result<f64, E>,
) -> Result<Zero, E> {
    if f_lower == 0.0 {
        return Ok(Zero {
            root: lower,
            iterations: Some(0),
            estimated_precision: 0.0,
        });
    }
    if f_upper == 0.0 {
        return Ok(Zero {
            root: upper,
            iterations: Some(0),
            estimated_precision: 0.0,
        });
    }

    let eps = f64::EPSILON;
    let max_evals = maxiter + 1;
    let mut evals = 0usize;

    // b is the current best estimate, a the previous one, c the opposite end of the bracket.
    let (mut a, mut b) = (lower, upper);
    let (mut fa, mut fb) = (f_lower, f_upper);
    let (mut c, mut fc) = (a, fa);

    loop {
        // Keep b the point with the smaller |f|.
        if fc.abs() < fb.abs() {
            a = b;
            b = c;
            c = a;
            fa = fb;
            fb = fc;
            fc = fa;
        }

        let tol1 = 2.0 * eps * b.abs() + 0.5 * tol;
        let half = 0.5 * (c - b);
        if half.abs() <= tol1 || fb == 0.0 {
            return Ok(Zero {
                root: b,
                iterations: Some(evals),
                estimated_precision: (c - b).abs(),
            });
        }
        if evals >= max_evals {
            return Ok(Zero {
                root: b,
                iterations: None,
                estimated_precision: (c - b).abs(),
            });
        }

        // The reference results measure the previous step as b - a, the step last taken (after the swap above),
        // where netlib's zeroin.f keeps the step before last in e.
        let last = b - a;
        let mut d = half;
        if last.abs() >= tol1 && fa.abs() > fb.abs() {
            let s = fb / fa;
            let (mut p, mut q);
            if a == c {
                // secant step
                p = 2.0 * half * s;
                q = 1.0 - s;
            } else {
                // inverse quadratic interpolation
                let qa = fa / fc;
                let rb = fb / fc;
                p = s * (2.0 * half * qa * (qa - rb) - (b - a) * (rb - 1.0));
                q = (qa - 1.0) * (rb - 1.0) * (s - 1.0);
            }
            if p > 0.0 {
                q = -q;
            } else {
                p = -p;
            }
            // Accept the interpolated step only if it stays well inside the bracket and shrinks fast enough.
            if 2.0 * p < 3.0 * half * q - (tol1 * q).abs() && p < (0.5 * last * q).abs() {
                d = p / q;
            }
        }

        a = b;
        fa = fb;
        if d.abs() > tol1 {
            b += d;
        } else if half > 0.0 {
            b += tol1;
        } else {
            b -= tol1;
        }
        fb = f(b)?;
        evals += 1;

        if fb * (fc / fc.abs()) > 0.0 {
            c = a;
            fc = fa;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::convert::Infallible;

    #[test]
    fn minimizes_a_parabola() {
        let x = fmin(0.0, 1.0, DEFAULT_TOL, |x| {
            Ok::<_, Infallible>((x - 1.0 / 3.0) * (x - 1.0 / 3.0))
        })
        .unwrap();
        assert!((x - 1.0 / 3.0).abs() < 1e-4);
    }

    #[test]
    fn finds_sqrt_two() {
        let z = zeroin(0.0, 2.0, -2.0, 2.0, 1e-12, 1000, |x| {
            Ok::<_, Infallible>(x * x - 2.0)
        })
        .unwrap();
        assert!((z.root - 2f64.sqrt()).abs() < 1e-11);
        assert!(z.iterations.is_some());
    }

    #[test]
    fn root_at_endpoint_and_exhaustion() {
        let z = zeroin(1.0, 3.0, 0.0, 2.0, DEFAULT_TOL, 1000, |x| {
            Ok::<_, Infallible>(x - 1.0)
        })
        .unwrap();
        assert_eq!(
            (z.root, z.iterations, z.estimated_precision),
            (1.0, Some(0), 0.0)
        );
        let z = zeroin(2.0, 3.0, -1.0, 16.0, 1e-15, 1, |x| {
            Ok::<_, Infallible>(x * x * x - 2.0 * x - 5.0)
        })
        .unwrap();
        assert_eq!(z.iterations, None);
    }

    #[test]
    fn errors_stop_the_search() {
        let mut calls = 0;
        let r = fmin(0.0, 1.0, DEFAULT_TOL, |_| {
            calls += 1;
            if calls == 3 { Err("stop") } else { Ok(1.0) }
        });
        assert_eq!((r, calls), (Err("stop"), 3));
    }
}
