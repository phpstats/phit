# phit

Fit regressions directly in PHP. This native extension uses Rust for estimation
and returns ordinary associative arrays with coefficients, uncertainty, fitted
values, and model statistics.

**[Documentation and math walkthroughs](https://phpstats.github.io/phit/)** ·
[PHP API](https://phpstats.github.io/phit/contracts.html) · [Installation](https://phpstats.github.io/phit/development.html) ·
[Verification](https://phpstats.github.io/phit/verification.html)

```php
use function Phit\fit_ols;

// Simulated apartments: floor area (m²) predicts monthly rent.
$result = fit_ols(
    x: [[74.4], [57.4], [66.4], [49.1], [45.7], [92.4], [37.0], [93.9]],
    y: [1879.25, 1805.03, 2022.29, 1267.85, 1509.24, 2240.10, 959.51, 2386.54],
);

$result['coefficients']; // [372.478545..., 21.479724...] — intercept, area
$result['std_errors'];   // Standard error for each coefficient
$result['conf_int'];     // 95% intervals: [[lower, upper], ...]
$result['r_squared'];    // Approximately 0.876
```

Supply one row per observation and one column per predictor. **The fixed intercept
is added automatically** and returned first. Use `intercept: false` for a model
through the origin, or to supply a complete design matrix yourself. Every
function lives in the `Phit` namespace: import it with `use function Phit\fit_ols;`
or call it as `Phit\fit_ols()`. Names are grouped by prefix: `fit_*` for models,
`dist_*` for distributions, `stat_*` for descriptive statistics and `draws_*` for
posterior draws.

## Models

| Function | Model |
| --- | --- |
| `fit_ols()` | Continuous outcome; ordinary or weighted least squares |
| `fit_logistic()` | Binary/grouped-binomial outcome; logit link |
| `fit_probit()` | Binary/grouped-binomial outcome; probit link |
| `fit_poisson()` | Counts; log link, weights and offsets |
| `fit_mixed_intercept()` | Gaussian random intercept; optimized ML fit |
| `fit_mixed()` | Gaussian mixed model; ML/REML, random slopes, multiple grouping terms |
| `fit_logistic_mixed()` | Binomial random intercept; Laplace or adaptive quadrature |
| `fit_lm()`, `fit_glm()` | R formulas over named columns; OLS, logit, probit and Poisson cores |
| `fit_lmer()`, `fit_glmer()` | lme4-style formulas over the mixed-model cores |
| `model_matrix()` | R-compatible `model.matrix()` with names, contrasts and random-effect designs |

The formula functions accept R syntax such as `'log(rent) ~ log(area) + district'`
and return coefficient names with the matrix function's results. See the
[formula guide](docs/formulas.qmd).

Each fitter has its own documented inference and diagnostics. General Gaussian
mixed models support explicit correlated/independent random designs; binomial
mixed models currently support one grouping factor and a random intercept.
See [supported structures and numerical limits](https://phpstats.github.io/phit/contracts.html).

## Probability and descriptive statistics

Evaluate normal, t, chi-square, F, beta, gamma, exponential, lognormal, logistic,
binomial, Poisson and negative-binomial distributions. Each has scalar and native
batch density/mass, CDF, survival, log-tail and quantile functions. For example:

```php
$upperTail = Phit\dist_normal_sf(10); // About 7.62e-24.
$logLikelihood = array_sum(Phit\dist_poisson_logpmf_array([2, 5, 3, 0], lambda: 2.5));
$quartiles = Phit\stat_quantile_array([2.5, 3.1, 4.7, 1.2, 9.8], [.25, .5, .75]);
```

Ten descriptive utilities provide mean, variance, SD, covariance/correlation,
type-7 quantiles, median and weighted moments. These utilities reject missing
values by default; dropping is explicit. Read the [distribution API](docs/distributions.qmd),
[descriptive conventions](docs/descriptive.qmd) and [v0.2 verification/readiness](docs/v0.2.qmd).
Seeded batch sampling uses `dist_{family}_rng()`, for example
`dist_normal_rng(n: 5, seed: 42)`. Every call owns its stream; see the
[sampling contract](docs/rng.qmd) for reproducibility and limits.

## Posterior draws

Three numerical kernels accept ordinary row-oriented PHP arrays:
`draws_moments()` returns a weighted mean and covariance,
`draws_covariance_map()` returns a Cholesky covariance transformation, and
`draws_affine()` applies a matrix and shift while preserving draw order.
Maps and affine transformations return `log_abs_det`. Draw inputs always reject
null/NAN/INF, with no row deletion or intercept insertion. Unbiased covariance
uses reliability weights. See the [numerical notes](docs/draw-numerics.qmd) for
shapes, tolerances, examples and independent verification. Stan evaluation, PSIS
and moment-matching control belong to the consuming CmdStan interface.

## Reading CSV files

`read_csv($path, $options)` reads delimited text into typed columns with
readr's options, type guessing, and problem reports. It is the native reader
behind phibble's `DataFrame::fromCsv()` and matches phibble's PHP reader exactly.
A 1,000,000-row file reads in about 0.1 seconds. See the
[CSV contract](docs/io.qmd).

## Poisson counts with exposure

Different observation periods belong in an offset on the **log scale**:

```php
$visitors = [82, 104, 96, 131, 87, 115, 142, 98];
$days = [1, 2, 1, 3, 2, 1, 2, 3];
$calls = [5, 12, 4, 22, 8, 8, 19, 16];

$result = Phit\fit_poisson(
    x: array_map(fn($v) => [$v], $visitors),
    y: $calls,
    offset: array_map(fn($d) => log($d), $days),
);

$expectedCount = $result['fitted'][0];
$expectedRate = $expectedCount / $days[0];
```

The model is `log(expected count) = intercept + predictors × coefficients + offset`.
The offset's coefficient is fixed at 1. Exposure must be positive; `weights:`
means positive prior likelihood weights and serves a different purpose. Offset
and weight arrays must align with the original observations. See the complete
[exposure example](examples/poisson-offset.php).

## Build and run

Requires **PHP 8.3+**, matching PHP development headers, stable Rust, and Clang.
Follow the [macOS/Linux setup guide](https://phpstats.github.io/phit/development.html) first, especially when
selecting `php`, `php-config`, and libclang. In that configured shell:

```sh
cargo build -p phit --release --locked

# macOS; use libphit.so on Linux.
export STATS_EXTENSION="$PWD/target/release/libphit.dylib"
php -n -d "extension=$STATS_EXTENSION" examples/ols.php
php -n -d "extension=$STATS_EXTENSION" examples/poisson-offset.php
```

The `-d` flag loads the extension for one run. To load it on every run, add
`extension=$PWD/target/release/libphit.dylib`, with the path written out in
full, to the php.ini that `php --ini` reports.

For a real application, install the compiled library and enable it in the
application runtime's PHP configuration as described in
[installation and deployment](https://phpstats.github.io/phit/development.html#running-in-an-application).
Then call `fit_ols()`, `fit_poisson()`, etc. normally. The
[PHP stubs](stubs/phit.php) support IDEs and static analysis; do not include
them at runtime.

## Server installation and containers

The [deployment guide](https://phpstats.github.io/phit/deployment.html)
explains PIE, PHP-FPM, Docker and versioned binary/source artifacts.

```sh
docker build --target checks -f docker/Dockerfile -t phit:checks .
docker build -f docker/Dockerfile -t phit:local .
./scripts/check-container phit:local phit:checks
```

The runtime image has the native extension enabled; use it as your application's
PHP-FPM base. The registered extension name is `phit`. PIE support is prepared
for local checkouts; public Packagist installation and tagged releases are pending.

## Data and errors

Inputs are numeric PHP arrays. Null and `NAN` values are dropped listwise with a
warning by default; `missing: 'raise'` rejects them. Results record retained and
dropped row positions. Invalid shapes, infinite values, rank deficiency, and
failed estimation raise exceptions with documented error codes. Fitted values
and residuals follow retained-row order.

**Migration:** older calls supplied their own ones column. Either remove that
column and use the default, or add `intercept: false` to preserve the previous
matrix exactly. Automatic insertion applies to fixed effects only; `random[*].z`
remains an explicit random-effect design.

## Tests and learning

```sh
cargo test --workspace --locked
PHP=/path/to/php PHP_CONFIG=/matching/php-config ./scripts/check
```

The release gate checks Rust, native PHP signatures, integration suites, and
examples. Deterministic fixtures compare appropriate quantities against R,
statsmodels, lme4, and independent numerical integration. Tests need neither R
nor Python for fitting; provenance and tolerances are documented in the
[verification record](https://phpstats.github.io/phit/verification.html).

The [Quarto walkthroughs](https://phpstats.github.io/phit/notebooks/index.html) explain matrices, estimation,
likelihood, and inference with simulated and published data. Figures use a shared
[R/ggplot2 strategy](https://phpstats.github.io/phit/visualization.html). R/Python are development tools;
PHP runtime computation uses Rust-native nalgebra.

The v0.1 fitting and packaging baseline is a **release candidate**, with the
v0.2 numerical additions documented in the [readiness assessment](docs/v0.2.qmd).
Read the
[stabilization assessment](https://phpstats.github.io/phit/v0.1.html) for verified scope and remaining release
gates.

## License

[MIT](LICENSE). Bundled reference datasets and third-party assets retain their
original licenses; see [licensing](https://phpstats.github.io/phit/licensing.html).
