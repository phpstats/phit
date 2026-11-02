# Linear binning, interpolation, the normal density, and the type-7 quantile

## Sources

- M. P. Wand, "Fast computation of multivariate kernel estimators", *Journal of Computational and Graphical
  Statistics* 3 (1994), 433–445, for linear binning.
- R's help for `density` ("The algorithm used in density.default disperses the mass of the empirical distribution
  function over a regular grid of at least 512 points"), `approx`, `dnorm`, and `quantile`.
- R. J. Hyndman and Y. Fan, "Sample quantiles in statistical packages", *The American Statistician* 50 (1996),
  361–365, definition 7.

## Interface

```rust
/// Linear binning of weighted points onto n equally spaced grid points from lo to hi (n >= 2, lo < hi), returned
/// zero-padded to length 2n.
pub fn linear_binning(x: &[f64], w: &[f64], lo: f64, hi: f64, n: usize) -> Vec<f64>;

pub enum Method { Linear, Constant(f64) }

/// Interpolation at v from points (x[i], y[i]) with x strictly increasing (at least 2 points). Values left of x[0]
/// are `low`; values right of the last x are `high`.
pub fn interpolate(x: &[f64], y: &[f64], v: f64, method: &Method, low: f64, high: f64) -> f64;

/// The standard normal density φ(z) = exp(-z²/2) / sqrt(2π).
pub fn standard_normal_density(z: f64) -> f64;

/// The type-7 sample quantile of sorted, finite data (at least one value) at probability p in [0, 1].
pub fn quantile_type7(sorted: &[f64], p: f64) -> f64;
```

## linear_binning

The grid points are `g_k = lo + kΔ` for `k = 0..n-1`, with `Δ = (hi - lo) / (n - 1)`.

- A non-finite x contributes nothing.
- For a finite x, let `t = (x - lo) / Δ`, `i = floor(t)`, and `f = t - i`. The point adds `w(1 - f)` to bin i and
  `w·f` to bin `i + 1`. An addition to a bin outside `0..n-1` is dropped. So a point just outside the grid still adds
  its share to the nearest end bin, and a point further out adds nothing.
- Points are added in input order.
- The result has length 2n. Bins n to 2n-1 are zero.

There is no R table for this routine. Check it with your own cases:
- The weight of points inside the grid is conserved.
- A point exactly on a grid point puts all its weight in that bin.
- Points outside behave as stated above.

## interpolate

This follows R's `approx` for x already sorted and without ties.

- If v is NaN, return NaN.
- If `v < x[0]`, return `low`. If v is greater than the last x, return `high`.
- If v equals some `x[j]`, return `y[j]`.
- Otherwise v lies strictly between neighbours `x[i] < v < x[i+1]`.
  - `Linear`: return `y[i] + (y[i+1] - y[i]) * ((v - x[i]) / (x[i+1] - x[i]))`.
  - `Constant(f)`: R documents that "if y0 and y1 are the values to the left and right of the point then the value
    is y0 if f == 0, y1 if f == 1, and y0*(1-f)+y1*f for intermediate values. In this way the result is
    right-continuous for f == 0 and left-continuous for f == 1, even for non-finite y values." A term whose
    coefficient is zero is left out, so an infinite neighbour does not produce NaN.

`data/approx.json` holds R's `approx()` results:
- The method and its `f` are given.
- `rule = 1` means `low = high = NaN`.
- `rule = 2` means `low = y[0]` and `high = y[last]`.
- The outputs are `%.17g` strings, and `"NaN"`/`"NA"` mean NaN.

The results must agree to 1e-15 relative and match exactly at the data points.

## standard_normal_density

`φ(z) = exp(-z²/2) / sqrt(2π)`:
- It returns 0 for infinite z, and NaN for NaN.
- It is symmetric in z.

The relative error must be at most 1e-15 for |z| up to 38, wherever the result is a normal (not subnormal) double.
Beyond that it must not be larger than the true value's nearest double by more than 1e-15 relative, and it may
underflow to subnormals and then zero as the true value does. The plain formula loses accuracy for large |z|, because
the rounding error of `z²` is magnified by the exponential. Compute `z²/2` with its rounding error accounted for, for
example by splitting z into a high part with few significant bits and a remainder.

`data/dnorm.json` holds R's `dnorm(z)`. Its `scaled` part holds `dnorm(x, 0, sigma)`, which phit computes as
`standard_normal_density(x / sigma) / sigma`. Report the largest relative difference. It must be at most 1e-14.

## quantile_type7

Hyndman and Fan's definition 7:
- With n sorted values `x_1 ≤ … ≤ x_n`, let `h = (n - 1)p + 1`, `j = floor(h)`, and `γ = h - j`.
- The quantile is `x_j + γ(x_{j+1} - x_j)`. Compute it as `(1 - γ)x_j + γ·x_{j+1}`, except return `x_j` exactly when
  `γ == 0` or `x_{j+1} == x_j`.
- At `p = 1`, `j = n`, and the quantile is `x_n`.

`data/quantile7.json` holds R's `quantile(x, p, type = 7)` on unsorted x. Sort first. The results must agree to 1e-15
relative.
