//! One-dimensional minimization and root finding: ports of R's C Brent_fmin
//! (behind stats::optimize) and R_zeroin2 (behind stats::uniroot).
//!
//! Both follow R's code step for step, so the same function gives the same
//! evaluation points and results. The objective is a fallible closure, so a
//! caller's error (such as a PHP exception) stops the search. Callers
//! validate the interval and tolerance and map nonfinite values as R's
//! wrappers do; these functions assume finite inputs.

/// R's default tolerance, .Machine$double.eps^0.25.
pub const DEFAULT_TOL: f64 = 0.000_122_070_312_5;

/// The minimizer of f on [lower, upper] by Brent's golden-section and
/// parabolic-interpolation search, as R's Brent_fmin.
pub fn fmin<E>(
    lower: f64,
    upper: f64,
    tol: f64,
    mut f: impl FnMut(f64) -> Result<f64, E>,
) -> Result<f64, E> {
    // The squared inverse of the golden ratio.
    let c = (3.0 - 5f64.sqrt()) * 0.5;
    let eps = f64::EPSILON.sqrt();
    let (mut a, mut b) = (lower, upper);
    let mut v = a + c * (b - a);
    let (mut w, mut x) = (v, v);
    let (mut d, mut e) = (0.0f64, 0.0f64);
    let mut fx = f(x)?;
    let (mut fv, mut fw) = (fx, fx);
    let tol3 = tol / 3.0;
    loop {
        let xm = (a + b) * 0.5;
        let tol1 = eps * x.abs() + tol3;
        let t2 = tol1 * 2.0;
        if (x - xm).abs() <= t2 - (b - a) * 0.5 {
            break;
        }
        let (mut p, mut q, mut r) = (0.0f64, 0.0f64, 0.0f64);
        if e.abs() > tol1 {
            // Fit a parabola.
            r = (x - w) * (fx - fv);
            q = (x - v) * (fx - fw);
            p = (x - v) * q - (x - w) * r;
            q = (q - r) * 2.0;
            if q > 0.0 {
                p = -p;
            } else {
                q = -q;
            }
            r = e;
            e = d;
        }
        if p.abs() >= (q * 0.5 * r).abs() || p <= q * (a - x) || p >= q * (b - x) {
            // A golden-section step.
            e = if x < xm { b - x } else { a - x };
            d = c * e;
        } else {
            // A parabolic-interpolation step, kept away from the ends.
            d = p / q;
            let u = x + d;
            if u - a < t2 || b - u < t2 {
                d = if x >= xm { -tol1 } else { tol1 };
            }
        }
        // f is never evaluated within tol1 of x.
        let u = if d.abs() >= tol1 {
            x + d
        } else if d > 0.0 {
            x + tol1
        } else {
            x - tol1
        };
        let fu = f(u)?;
        if fu <= fx {
            if u < x {
                b = x;
            } else {
                a = x;
            }
            v = w;
            w = x;
            x = u;
            fv = fw;
            fw = fx;
            fx = fu;
        } else {
            if u < x {
                a = u;
            } else {
                b = u;
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
    Ok(x)
}

/// The result of [`zeroin`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Zero {
    pub root: f64,
    /// Iterations used, or None when maxiter ran out first.
    pub iterations: Option<usize>,
    /// |c - b| at the end, R's estim.prec; 0 for a root at an endpoint.
    pub estimated_precision: f64,
}

/// A root of f in [lower, upper], given f at both ends with opposite signs,
/// by Brent's method as R's R_zeroin2.
pub fn zeroin<E>(
    lower: f64,
    upper: f64,
    f_lower: f64,
    f_upper: f64,
    tol: f64,
    maxiter: usize,
    mut f: impl FnMut(f64) -> Result<f64, E>,
) -> Result<Zero, E> {
    let (mut a, mut b) = (lower, upper);
    let (mut fa, mut fb) = (f_lower, f_upper);
    let (mut c, mut fc) = (a, fa);
    let endpoint = |root| Zero {
        root,
        iterations: Some(0),
        estimated_precision: 0.0,
    };
    if fa == 0.0 {
        return Ok(endpoint(a));
    }
    if fb == 0.0 {
        return Ok(endpoint(b));
    }
    for used in 0..=maxiter {
        let prev_step = b - a;
        if fc.abs() < fb.abs() {
            // Make b the best approximation.
            a = b;
            b = c;
            c = a;
            fa = fb;
            fb = fc;
            fc = fa;
        }
        let tol_act = 2.0 * f64::EPSILON * b.abs() + tol / 2.0;
        let mut new_step = (c - b) / 2.0;
        if new_step.abs() <= tol_act || fb == 0.0 {
            return Ok(Zero {
                root: b,
                iterations: Some(used),
                estimated_precision: (c - b).abs(),
            });
        }
        // Try interpolation when the previous step was large enough and in
        // the right direction.
        if prev_step.abs() >= tol_act && fa.abs() > fb.abs() {
            let cb = c - b;
            let (mut p, mut q);
            if a == c {
                // Two distinct points: linear interpolation.
                let t1 = fb / fa;
                p = cb * t1;
                q = 1.0 - t1;
            } else {
                // Inverse quadratic interpolation.
                q = fa / fc;
                let t1 = fb / fc;
                let t2 = fb / fa;
                p = t2 * (cb * q * (q - t1) - (b - a) * (t1 - 1.0));
                q = (q - 1.0) * (t1 - 1.0) * (t2 - 1.0);
            }
            if p > 0.0 {
                q = -q;
            } else {
                p = -p;
            }
            if p < 0.75 * cb * q - (tol_act * q).abs() / 2.0 && p < (prev_step * q / 2.0).abs() {
                new_step = p / q;
            }
        }
        if new_step.abs() < tol_act {
            new_step = if new_step > 0.0 { tol_act } else { -tol_act };
        }
        a = b;
        fa = fb;
        b += new_step;
        fb = f(b)?;
        if (fb > 0.0 && fc > 0.0) || (fb < 0.0 && fc < 0.0) {
            // Keep c on the other side of the root from b.
            c = a;
            fc = fa;
        }
    }
    Ok(Zero {
        root: b,
        iterations: None,
        estimated_precision: (c - b).abs(),
    })
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
