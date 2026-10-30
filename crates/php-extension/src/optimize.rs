//! PHP bindings for one-dimensional minimization and root finding, as R's
//! stats::optimize and stats::uniroot.
//!
//! f is any PHP callable taking one float and returning an int or float. An
//! exception thrown by f stops the search and propagates unchanged. As in R,
//! a nonfinite value of f during the search is replaced (NAN and INF by
//! PHP_FLOAT_MAX, and for uniroot -INF by -PHP_FLOAT_MAX) with one E_WARNING
//! per call; the returned objective and f_root are f's own values.
use crate::distributions::Real;
use crate::stats_exception;
use ext_php_rs::{
    boxed::ZBox, error::Error, prelude::*, types::ZendCallable, types::ZendHashTable,
    zend::ExecutorGlobals,
};
use stats_core::optimize::{self, DEFAULT_TOL};
use stats_core::{ErrorKind, StatsError};

fn invalid(message: &str) -> PhpException {
    stats_exception(StatsError::new(ErrorKind::InvalidInput, message))
}

/// Why a search stopped early.
enum Stop {
    /// f threw; the engine holds its exception.
    Thrown,
    Error(PhpException),
}

fn call(f: &ZendCallable, x: f64, name: &str) -> Result<f64, Stop> {
    match f.try_call(vec![&x]) {
        Ok(value) => value
            .double()
            .or_else(|| value.long().filter(|_| value.is_long()).map(|v| v as f64))
            .filter(|_| value.is_double() || value.is_long())
            .ok_or_else(|| {
                Stop::Error(invalid(&format!("{name}(): f must return an int or float")))
            }),
        Err(Error::ExceptionPending { .. }) => Err(Stop::Thrown),
        Err(_) if ExecutorGlobals::has_exception() => Err(Stop::Thrown),
        Err(error) => Err(Stop::Error(error.into())),
    }
}

// Warns once per search that nonfinite values were replaced, as R warns.
struct Replacements {
    warned: bool,
}

impl Replacements {
    fn note(&mut self) {
        if !self.warned {
            self.warned = true;
            ext_php_rs::error::php_error(
                &ext_php_rs::flags::ErrorType::Warning,
                "nonfinite function values were replaced by the largest finite value",
            );
        }
    }
}

fn tolerance(tol: Option<f64>) -> PhpResult<f64> {
    let tol = tol.unwrap_or(DEFAULT_TOL);
    if !tol.is_finite() || tol <= 0.0 {
        return Err(invalid("tol must be a positive finite number"));
    }
    Ok(tol)
}

fn interval(lower: f64, upper: f64) -> PhpResult<()> {
    if !lower.is_finite() || !upper.is_finite() {
        return Err(invalid("lower and upper must be finite"));
    }
    if lower >= upper {
        return Err(invalid("lower must be less than upper"));
    }
    Ok(())
}

// An exception from f is already pending: return to PHP so it propagates.
fn stopped(stop: Stop) -> PhpResult<ZBox<ZendHashTable>> {
    match stop {
        Stop::Thrown => Ok(ZendHashTable::new()),
        Stop::Error(error) => Err(error),
    }
}

/// Minimize (or maximize) f on [lower, upper], as R's optimize().
#[php_function]
#[php(name = "Phit\\optimize", defaults(maximum = false))]
pub fn optimize(
    f: ZendCallable,
    lower: Real,
    upper: Real,
    tol: Option<Real>,
    maximum: bool,
) -> PhpResult<ZBox<ZendHashTable>> {
    const NAME: &str = "Phit\\optimize";
    let (lower, upper) = (lower.0, upper.0);
    interval(lower, upper)?;
    let tol = tolerance(tol.map(|t| t.0))?;
    let mut replaced = Replacements { warned: false };
    let search = optimize::fmin(lower, upper, tol, |x| {
        let y = call(&f, x, NAME)?;
        let y = if maximum { -y } else { y };
        Ok(if y.is_finite() {
            y
        } else {
            replaced.note();
            f64::MAX
        })
    });
    let x = match search {
        Ok(x) => x,
        Err(stop) => return stopped(stop),
    };
    let objective = match call(&f, x, NAME) {
        Ok(y) => y,
        Err(stop) => return stopped(stop),
    };
    let mut out = ZendHashTable::new();
    out.insert(if maximum { "maximum" } else { "minimum" }, x)?;
    out.insert("objective", objective)?;
    Ok(out)
}

/// A root of f in [lower, upper], where f changes sign, as R's uniroot().
#[php_function]
#[php(name = "Phit\\uniroot", defaults(maxiter = 1000))]
pub fn uniroot(
    f: ZendCallable,
    lower: Real,
    upper: Real,
    tol: Option<Real>,
    maxiter: i64,
) -> PhpResult<ZBox<ZendHashTable>> {
    const NAME: &str = "Phit\\uniroot";
    let (lower, upper) = (lower.0, upper.0);
    interval(lower, upper)?;
    let tol = tolerance(tol.map(|t| t.0))?;
    if maxiter < 1 {
        return Err(invalid("maxiter must be at least 1"));
    }
    let ends = (call(&f, lower, NAME), call(&f, upper, NAME));
    let (f_lower, f_upper) = match ends {
        (Ok(a), Ok(b)) => (a, b),
        (Err(stop), _) | (_, Err(stop)) => return stopped(stop),
    };
    if f_lower.is_nan() || f_upper.is_nan() {
        return Err(invalid("f is NAN at lower or upper"));
    }
    let sign = |v: f64| {
        if v > 0.0 {
            1.0
        } else if v < 0.0 {
            -1.0
        } else {
            0.0
        }
    };
    if sign(f_lower) * sign(f_upper) > 0.0 {
        return Err(invalid("f must have opposite signs at lower and upper"));
    }
    let truncate = |v: f64| v.clamp(-f64::MAX, f64::MAX);
    let mut replaced = Replacements { warned: false };
    let search = optimize::zeroin(
        lower,
        upper,
        truncate(f_lower),
        truncate(f_upper),
        tol,
        maxiter as usize,
        |x| {
            let y = call(&f, x, NAME)?;
            Ok(if y.is_finite() {
                y
            } else {
                replaced.note();
                if y == f64::NEG_INFINITY {
                    -f64::MAX
                } else {
                    f64::MAX
                }
            })
        },
    );
    let zero = match search {
        Ok(zero) => zero,
        Err(stop) => return stopped(stop),
    };
    let iterations = match zero.iterations {
        Some(n) => n as i64,
        None => {
            ext_php_rs::error::php_error(
                &ext_php_rs::flags::ErrorType::Warning,
                &format!("not converged in {maxiter} iterations"),
            );
            maxiter
        }
    };
    let f_root = match call(&f, zero.root, NAME) {
        Ok(y) => y,
        Err(stop) => return stopped(stop),
    };
    let mut out = ZendHashTable::new();
    out.insert("root", zero.root)?;
    out.insert("f_root", f_root)?;
    out.insert("iter", iterations)?;
    out.insert("estim_prec", zero.estimated_precision)?;
    Ok(out)
}

pub fn register(module: ModuleBuilder) -> ModuleBuilder {
    module
        .function(wrap_function!(optimize))
        .function(wrap_function!(uniroot))
}
