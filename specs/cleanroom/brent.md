# One-dimensional minimization and root finding

## Sources

- R. P. Brent, *Algorithms for Minimization without Derivatives*, Prentice-Hall, 1973, chapters 4 (zero) and 5
  (minimum).
- netlib `fmin.f` and `zeroin.f` (<https://www.netlib.org/go/>), public domain, which give Brent's ALGOL procedures in
  Fortran. Follow their arithmetic. The differences required by phit are stated below.

## Interface

```rust
/// R's default tolerance for optimize() and uniroot(): .Machine$double.eps^0.25.
pub const DEFAULT_TOL: f64 = 0.000_122_070_312_5;

/// A minimizer of f on [lower, upper] by Brent's method (golden section with parabolic interpolation).
/// Returns the final x. An error from f stops the search and is returned at once.
pub fn fmin<E>(lower: f64, upper: f64, tol: f64, f: impl FnMut(f64) -> Result<f64, E>) -> Result<f64, E>;

pub struct Zero {
    pub root: f64,
    /// Evaluations of f made by the search before convergence, or None when it stopped without converging.
    pub iterations: Option<usize>,
    /// The width of the final bracket, |c - b| (0 for a root at an endpoint).
    pub estimated_precision: f64,
}

/// A root of f in [lower, upper] by Brent's method, given f(lower) and f(upper), which have opposite signs or one
/// of which is zero. An error from f stops the search and is returned at once.
pub fn zeroin<E>(lower: f64, upper: f64, f_lower: f64, f_upper: f64, tol: f64, maxiter: usize,
                 f: impl FnMut(f64) -> Result<f64, E>) -> Result<Zero, E>;
```

Callers guarantee `lower < upper`, finite bounds, a positive finite `tol`, `maxiter >= 1`, and finite values returned by
`f`. phit's binding replaces non-finite values before they reach these routines.

## fmin

Follow netlib `fmin.f`, with these choices:

- The golden-section ratio is `c = (3 - sqrt(5)) / 2`.
- `eps` is `sqrt(f64::EPSILON)`. Do not compute machine epsilon by a loop.
- The first point is `v = w = x = a + c(b - a)`, with `d = e = 0`, and f is evaluated there once.
- At each step, `tol1 = eps * |x| + tol / 3` and `tol2 = 2 * tol1`.
- The search stops when `|x - xm| <= tol2 - (b - a) / 2`, where `xm = (a + b) / 2`, and returns `x`.
- Where a step `d` would land within `tol2` of `a` or `b`, or would be smaller than `tol1`, the step becomes `tol1`
  with a sign. Take that sign as negative when `x >= xm` in the first case, and when `d <= 0` in the second case.
  netlib's `sign(tol1, ·)` gives a positive step for a zero argument, and phit's reference results expect the
  negative one there.
- Each step evaluates f once, so the number of evaluations is 1 plus the number of steps.

The data in `data/optimize.json` (`optimize` cases) were produced by R's `optimize()`. These cases match the routine
directly:
- `parabola`, `sine`, `kink`, `double_well`, `far`, `decay`, `flat`, and `linear`;
- `hump` and the second `sine` case with `maximum: true`, where f is negated;
- `wall`, where f values of `+Inf` are replaced by `f64::MAX` before the routine sees them.

`x` and `evaluations` must match exactly, bit for bit. The functions are:

| name | f(x) |
| --- | --- |
| parabola | `(x - 1/3) * (x - 1/3)` |
| sine | `sin(x)` |
| kink | `abs(x - 0.7)` |
| flat | `1` |
| linear | `x` |
| decay | `exp(-x)` |
| double_well | `(x*x - 1) * (x*x - 1) + 0.3*x` |
| wall | `x < 0.45 ? +Inf : (x - 0.5) * (x - 0.5)` |
| far | `(x - 1e6) * (x - 1e6)` |
| hump | `x * exp(-x)` |

`tol` is `DEFAULT_TOL` unless a case gives one.

## zeroin

Follow netlib `zeroin.f`, with these choices:

- f is not evaluated at the endpoints, because the caller passes `f_lower` and `f_upper`.
- If `f_lower == 0`, return `lower` with `iterations: Some(0)` and `estimated_precision: 0`. Otherwise, if
  `f_upper == 0`, return `upper` the same way.
- Machine epsilon is `f64::EPSILON`. The tolerance at each step is `2 * EPSILON * |b| + tol / 2`.
- The search converges when the half-width `|c - b| / 2` is at most that tolerance, or when `f(b) == 0`. It then returns
  `b`, with `estimated_precision = |c - b|` and `iterations: Some(k)`, where k is the number of evaluations of f that
  the search has made.
- The search makes at most `maxiter + 1` evaluations. If it has not converged once that many are made, it returns the
  current `b` with `iterations: None` and `estimated_precision = |c - b|`.

The `uniroot` cases in `data/optimize.json` were produced by R's `uniroot()`. Replace a non-finite value of f, at
the endpoints and inside the search, by `f64::MAX` with its sign: `-inf` becomes `-f64::MAX`. phit's binding does the
same. `root`, `iter`, and `estim_prec` must match:
- For a converged case, `iter` is `iterations`.
- For the case with `converged: false`, `iterations` must be `None` and the root and precision must match.
- `evaluations` counts R's two endpoint calls and one more call at the root, so the routine's own count is
  `evaluations - 3`.

The functions are:

| name | f(x) |
| --- | --- |
| sqrt2 | `x*x - 2` |
| fixed_point | `cos(x) - x` |
| exp5 | `exp(x) - 5` |
| at_one | `x - 1` |
| at_three | `x - 3` |
| step | `x < 0.3 ? -1 : 1` |
| cubic | `x*x*x - 2*x - 5` |
| log | `ln(x)`, so `f(0) = -inf` |
| cliff | `x < 0.5 ? -inf : x - 0.6` |

Report every case that does not match bit for bit, with both values.
