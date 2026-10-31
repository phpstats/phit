# phit

phit is a PHP extension, written in Rust, for regression and mixed models,
probability distributions and descriptive statistics. Models are written as R
formulas over named columns, and results are plain PHP arrays.

**[Documentation](https://phit.phpstats.org/docs)** ·
[Installation](https://phit.phpstats.org/docs/installation) ·
[A first model](https://phit.phpstats.org/docs/getting-started) ·
[API overview](https://phit.phpstats.org/docs/contracts) ·
[Verification](https://phit.phpstats.org/docs/verification)

```php
use function Phit\{fit_lm, predict};

$flats = [
    'rent' => [1879.25, 1805.03, 2022.29, 1267.85, 1509.24, 2240.10, 959.51, 2386.54, 1655.80, 1420.00],
    'area' => [74.4, 57.4, 66.4, 49.1, 45.7, 92.4, 37.0, 93.9, 61.2, 52.3],
    'district' => ['north', 'south', 'north', 'east', 'south', 'north', 'east', 'south', 'east', 'south'],
];

$fit = fit_lm('rent ~ area + district', $flats);
$fit['named'];     // ['(Intercept)' => 352.64, 'area' => 19.18, 'districtnorth' => 203.64, 'districtsouth' => 232.16]
$fit['r_squared']; // 0.927

$p = predict($fit, ['area' => [55.0], 'district' => ['east']], interval: 'prediction');
// $p['fit'] [1407.55], $p['lower'] [991.59], $p['upper'] [1823.51]
```

## Install

```sh
pie install phpstats/phit
```

phit supports PHP 8.3, 8.4 and 8.5, non-thread-safe builds, on Linux and macOS.
On Linux x86_64 and arm64 with glibc 2.35 or later, [PIE](https://github.com/php/pie)
downloads a prebuilt binary from the
[release](https://github.com/phpstats/phit/releases). Elsewhere, including
macOS, it builds from source, which needs Rust, Clang and libclang, and PHP's
development files. The [installation guide](https://phit.phpstats.org/docs/installation)
covers both and how to check the install.

[phit-polyfill](https://github.com/phpstats/phit-polyfill)
(`composer require phpstats/phit-polyfill`) provides the same functions in
plain PHP, except the CSV reader, for hosts where extensions cannot be
installed.

## What it includes

| Area | Functions |
| --- | --- |
| Formula models | `fit_lm`, `fit_glm` (Gaussian, binomial logit and probit, Poisson), `fit_lmer`, `fit_glmer`, `model_matrix`, `predict` |
| Matrix models | `fit_ols`, `fit_logistic`, `fit_probit`, `fit_poisson`, `fit_mixed_intercept`, `fit_mixed`, `fit_logistic_mixed` |
| Distributions | `dist_*`: densities, CDFs, survival functions, quantiles and seeded samples |
| Descriptive statistics | `stat_*`: moments, quantiles, weighted statistics, kernel density, interpolation, ECDF |
| Posterior draws | `draws_moments`, `draws_covariance_map`, `draws_affine` |
| Optimization | `optimize`, `uniroot` |
| CSV input | `read_csv`, `CsvReader` |

All functions are in the `Phit` namespace. [stubs/phit.php](stubs/phit.php)
declares them for IDEs and static analysis; do not load it at runtime.

## Development

Building from a checkout, running the tests and the release process are
described in the [development guide](https://phit.phpstats.org/docs/development).
The complete check is:

```sh
PHP_CONFIG=/path/to/php-config ./scripts/check
```

## License

[MIT](LICENSE). Bundled reference datasets and third-party code keep their
original licenses; see [licensing](https://phit.phpstats.org/docs/licensing).
