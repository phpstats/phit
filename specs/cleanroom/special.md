# Saddle-point probabilities and incomplete gamma and beta functions

## Sources

- C. Loader, "Fast and accurate computation of binomial probabilities", 2000,
  <https://www.r-project.org/doc/reports/CLoader-dbinom-2002.pdf>. The paper describes the Stirling error δ, the
  deviance term, and the saddle-point forms of the binomial and Poisson probabilities. Its algorithms are the source
  here. The paper itself is the reference; do not use any program code.
- M. Abramowitz and I. A. Stegun, *Handbook of Mathematical Functions*: 6.5.29 (series for γ(a, x)), 6.5.31
  (continued fraction for Γ(a, x)), and 26.5.8 (continued fraction for I_x(a, b)).
- W. J. Lentz, "Generating Bessel functions in Mie scattering calculations using continued fractions", *Applied
  Optics* 15 (1976), 668–671. Also I. J. Thompson and A. R. Barnett, "Coulomb and Bessel functions of complex
  arguments and order", *Journal of Computational Physics* 64 (1986), 490–509. Together these give the modified
  Lentz method for evaluating continued fractions.

You may depend on the `statrs` crate (MIT) for `ln_gamma` and `ln_beta`, as phit does.

## Interface

```rust
/// δ(n) = ln Γ(n + 1) - (n + 1/2) ln n + n - ln √(2π), for n > 0.
pub fn stirling_error(n: f64) -> f64;

/// x ln(x / np) + np - x for x >= 0 and np > 0, accurate when x ≈ np (where both terms nearly cancel).
pub fn deviance_term(x: f64, np: f64) -> f64;

/// ln(λ^x e^-λ / Γ(x + 1)) for real x >= 0 and λ >= 0, by the saddle-point form.
pub fn ln_poisson_raw(x: f64, lambda: f64) -> f64;

/// ln(Γ(n + 1) / (Γ(x + 1) Γ(n - x + 1)) p^x q^(n-x)) for real 0 <= x <= n, with q = 1 - p given separately and
/// accurately by the caller.
pub fn ln_binomial_raw(x: f64, n: f64, p: f64, q: f64) -> f64;

/// As ln_binomial_raw with n = x + m, for callers that know m = n - x exactly.
pub fn ln_binomial_raw_split(x: f64, m: f64, p: f64, q: f64) -> f64;

/// ln P(a, x), the regularized lower incomplete gamma function, by its power series; used for 0 < x < a + 1.
pub fn ln_gamma_lower_series(a: f64, x: f64) -> f64;

/// ln Q(a, x), the regularized upper incomplete gamma function, by its continued fraction evaluated with the
/// modified Lentz method; used for x > a + 1.
pub fn ln_gamma_upper_fraction(a: f64, x: f64) -> f64;

/// ln I_x(a, b), the regularized incomplete beta function, by its continued fraction (modified Lentz); used for
/// 0 < x < (a + 1)/(a + b + 2). The caller passes ln x and ln(1 - x), computed without cancellation.
pub fn ln_beta_lower_fraction(a: f64, b: f64, x: f64, ln_x: f64, ln_y: f64) -> f64;
```

## Requirements

### stirling_error

- For n of at least about 15, use the asymptotic series in 1/n given in Loader's paper. Choose the number of terms so
  that the truncation error is below the double rounding.
- For smaller n, compute δ accurately in your own way. Avoid the cancellation of the defining formula. For example, at
  half-integers up to 15 you may use a table of exact values computed in high precision, which you generate yourself.
- The relative error must be at most 1e-14. Check it against `special.json` (`stirling_error`, pairs [n, δ(n)]).

### deviance_term

- When x is far from np, use the formula directly.
- When `|x - np|` is small relative to `x + np`, use the series in `(x - np)/(x + np)` from Loader's paper, summed
  until the terms no longer change the sum.
- For `x = 0`, the result is np.
- The relative error must be at most 1e-13. Check it against `special.json` (`deviance_term`, triples).

### ln_poisson_raw

- λ = 0 gives 0 for x = 0 and -∞ otherwise.
- x = 0 gives -λ.
- Otherwise use Loader's saddle-point form: `-δ(x) - D(x, λ) - ½ ln(2πx)`, where D is `deviance_term`.
- The error must be at most 1e-13 · max(1, |value|). Check it against `special.json` (`ln_poisson_raw`).

### ln_binomial_raw and ln_binomial_raw_split

- p = 0 gives 0 for x = 0 and -∞ otherwise. q = 0 gives 0 for x = n and -∞ otherwise.
- x = 0 gives n ln q. When p is small, q has lost the low digits of p, so compute this from p, for example
  `-D(n, nq) - np` or `n ln1p(-p)`. x = n gives n ln p in the same way.
- Otherwise use Loader's saddle-point form: the Stirling errors of n, x, and n - x, the deviance terms of x against np
  and of n - x against nq, and the term `½ ln(2π x (n - x)/n)`.
- The split form uses m wherever n - x appears and `x + m` for n.
- The error must be at most 1e-13 · max(1, |value|). Check it against `special.json` (`ln_binomial_raw`, which gives
  x, n, p, q, and the value).

### Incomplete gamma and beta

- Return the natural logarithm. The prefactor (`x^a e^-x / Γ(a + 1)` for the series, and the corresponding factors
  for the fractions) must be combined on the log scale, so the result stays finite and accurate when the value
  underflows.
- Stop each series or fraction when the relative change in the result falls below the double rounding, with an
  iteration cap you choose. Return NaN if the cap is reached.
- The error must be at most 1e-13 · max(1, |value|). Check them against `special.json` (`ln_gamma_lower`,
  `ln_gamma_upper`, and `ln_beta_lower`; the inputs are a, x or a, b, x).
- `ln_beta_lower_fraction` must use `ln_x` and `ln_y` for every logarithm of x and of 1 − x. The caller computes them
  without loss, but x itself may have underflowed. When x is 0 or subnormal, `ln_x` still carries the value: the
  result is then a·ln x + b·ln(1 − x) − ln(a·B(a, b)) plus the logarithm of the fraction, which is close to 1. A
  result of -∞ is wrong whenever `ln_x` is finite. Check this against `ln_beta_lower_tiny` in `special.json`, where
  each row gives a, b, x (0 there), ln x, ln(1 − x), and the value, with the same bound.
