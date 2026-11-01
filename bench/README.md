# Workload benchmarks

These scripts time one fit, or one batch of distribution functions, in the phit
extension, phit-polyfill, and two reference implementations. They write one
record per workload for the website, `site/benchmarks/{workload}-workload.json`.
The per-model benchmarks in `tests/benchmarks` are separate.

| Workload | Data | phit and phit-polyfill | R | Python |
| --- | --- | --- | --- | --- |
| `ols` | 1,000,000 rows, 10 predictors | `fit_ols()` | `lm()` | statsmodels `ols().fit()` |
| `mixed` | 20,000 rows in 200 groups, 3 predictors, random intercept, REML | `fit_mixed_gaussian()` | lme4 `lmer()` | statsmodels `mixedlm().fit(reml=True)` |
| `logistic` | 1,000,000 rows, 10 predictors, 0/1 response | `fit_generalized_linear(family: 'binomial')` | `glm(family = binomial)` | statsmodels `logit().fit(method='newton')` |
| `probit` | The logistic data | `fit_generalized_linear(link: 'probit')` | `glm(family = binomial("probit"))` | statsmodels `probit().fit(method='newton')` |
| `mixed-logistic` | 20,000 rows in 200 groups, 3 predictors, 0/1 response, random intercept, Laplace | `fit_mixed_logistic()` | lme4 `glmer()`, `tolPwrss = 1e-10` | GPBoost `GPModel(likelihood='bernoulli_logit')` |
| `distributions` | 1,000,000 inputs per function | `dist_normal_cdf_array()` and the quantile and gamma batch functions | `pnorm()`, `qnorm()`, `pgamma()`, `qgamma()` | SciPy `stats.norm`, `stats.gamma` |

## Method

- Each run is a fresh process. Each implementation runs once as a warm-up,
  then the four alternate for 7 rounds, in a rotated order each round. The
  record reports the median and every run.
- The time runs from the data already in memory to holding the coefficients
  and standard errors. The data are in each runtime's usual structure: PHP
  arrays of columns, an R `data.frame`, or a pandas `DataFrame`. The time
  includes building the model matrix from the formula. Reading the CSV file is
  outside the timing.
- Every process runs under `/usr/bin/time -l`. Memory is the process's peak
  memory footprint as macOS reports it, data included: its own allocations,
  without the clean file pages it maps. The record also gives the peak resident
  set size (`peak_rss_mib`), which counts those pages. The baseline is each
  runtime's peak footprint with its libraries loaded and no data.
- Every runtime uses one thread. The runner sets `OPENBLAS_NUM_THREADS`,
  `OMP_NUM_THREADS`, `MKL_NUM_THREADS`, `VECLIB_MAXIMUM_THREADS` and
  `BLIS_NUM_THREADS` to 1. phit and phit-polyfill are single-threaded.
- The runner records the largest coefficient difference from phit as a sanity
  check, or for `distributions` the largest relative difference from SciPy over
  every 1000th output. This is not a verification; the verification pages compare the models
  with R at stated tolerances.
- The record's `code` holds each script's timed lines, the lines between its
  `[workload]` and `[/workload]` comments, without their indentation.

Each size is set so the polyfill finishes in under a minute per run. Each record states the optimizers and their convergence settings.

## Running

The runner needs the release build of the extension, a phit-polyfill checkout
with `composer install` run, R with lme4, and a Python environment with
statsmodels, pandas, SciPy, and GPBoost. It runs on macOS, where `/usr/bin/time -l` reports the
peak memory footprint.

```sh
python3 -m venv .context/bench-venv
.context/bench-venv/bin/pip install statsmodels pandas numpy scipy gpboost==1.8.0
.context/bench-venv/bin/python bench/generate.py
.context/bench-venv/bin/python bench/run.py --polyfill ../phit-polyfill --polyfill-version 0.4.0
```

`generate.py` writes the data sets to `.context/bench-data` from fixed seeds.
`fit.php`, `fit.R` and `fit.py` each run one workload and print one JSON line.
`fit.php` times phit when the extension is loaded, and the polyfill when it is
not and `PHIT_POLYFILL_AUTOLOAD` names the polyfill's `vendor/autoload.php`.
