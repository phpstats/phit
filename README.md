# phit

phit is a PHP extension, written in Rust, for regression and mixed models,
probability distributions, and descriptive statistics. Models are written as R
formulas over named columns. A fit prints its summary in R's layout and reads like
a PHP array.

**[Documentation](https://phit.phpstats.org/docs)** ·
[Installation](https://phit.phpstats.org/docs/installation) ·
[A first model](https://phit.phpstats.org/docs/getting-started) ·
[API overview](https://phit.phpstats.org/docs/contracts) ·
[Verification](https://phit.phpstats.org/docs/verification)

```php
use Phibble\DataFrame;

use function Phit\{fit_ols, predict};

$cars = DataFrame::fromCsv(
    'https://raw.githubusercontent.com/phpstats/datasets/main/csv/mtcars.csv',
);

$fit = fit_ols('mpg ~ wt + factor(cyl)', $cars);
echo $fit;

$p = predict(
    $fit,
    ['wt' => [2.5], 'cyl' => [4]],
    interval: 'prediction',
);
// $p['fit'], $p['lower'], and $p['upper'] are lists, one value per row.
```

```
Linear model fit by least squares
Formula: mpg ~ wt + factor(cyl)

Coefficients:
             Estimate Std. Error t value Pr(>|t|)
(Intercept)   33.9908     1.8878  18.006  < 2e-16
wt            -3.2056     0.7539  -4.252 0.000213
factor(cyl)6  -4.2556     1.3861  -3.070 0.004718
factor(cyl)8  -6.0709     1.6523  -3.674 0.000999

Residual standard error: 2.557 on 28 degrees of freedom
Multiple R-squared: 0.8374, Adjusted R-squared: 0.82
F-statistic: 48.08 on 3 and 28 DF, p-value: 3.594e-11
```

The example reads [mtcars](https://github.com/phpstats/datasets#mtcars) with
[phibble](https://github.com/phpstats/phibble)
(`composer require phpstats/phibble`).

## Installation

```sh
pie install phpstats/phit
```

phit supports PHP 8.3, 8.4, and 8.5, non-thread-safe builds, on Linux and macOS.
On Linux x86_64 and arm64 with glibc 2.35 or later, [PIE](https://github.com/php/pie)
downloads a prebuilt binary from the
[release](https://github.com/phpstats/phit/releases). Elsewhere, including
macOS, it builds from source, which needs Rust, Clang, libclang, and PHP's
development files. The [installation guide](https://phit.phpstats.org/docs/installation)
covers both and how to check the install.

[phit-polyfill](https://github.com/phpstats/phit-polyfill)
(`composer require phpstats/phit-polyfill`) provides the same functions in
plain PHP, except the CSV reader (`read_csv` and `CsvReader`), for hosts where
extensions cannot be installed.

## Features

| Area | Functions |
| --- | --- |
| Formula models | `fit_ols`, `fit_generalized_linear` (Gaussian, binomial logit and probit, and Poisson), `fit_mixed_gaussian`, `fit_mixed_logistic`, `model_matrix`, and `predict` |
| Instrumental variables | `fit_iv`: weighted two-stage least squares with weak-instrument and endogeneity diagnostics |
| Ridge regression | `fit_ridge` and `cv_ridge`: penalties, coefficient bounds, and weighted cross-validation |
| Printing results | `summary` (R's `print(summary(fit))` layout) and `tidy` (broom-style coefficient and random-effect columns) |
| Matrix models | `fit_ols_matrix`, `fit_logistic_matrix`, `fit_probit_matrix`, `fit_poisson_matrix`, `fit_mixed_gaussian_intercept_matrix`, `fit_mixed_gaussian_matrix`, and `fit_mixed_logistic_matrix` |
| Distributions | `dist_*`: densities, CDFs, survival functions, quantiles, and seeded samples |
| Descriptive statistics | `stat_*`: moments, quantiles, weighted statistics, kernel density, interpolation, and the ECDF |
| Posterior draws | `draws_moments`, `draws_covariance_map`, and `draws_affine` |
| Optimization | `optimize` and `uniroot` |
| CSV input | `read_csv` and `CsvReader` |

All functions are in the `Phit` namespace. [stubs/phit.php](stubs/phit.php)
declares them for IDEs and static analysis; do not load it at runtime.

## Development

Building from a checkout, running the tests, and the release process are
described in the [development guide](https://phit.phpstats.org/docs/development).
The complete check is:

```sh
PHP_CONFIG=/path/to/php-config ./scripts/check
```

## License

[MIT](LICENSE). Bundled reference datasets and third-party code keep their
original licenses; see [licensing](https://phit.phpstats.org/docs/licensing).
[THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md) lists the third-party software
and its licenses.
