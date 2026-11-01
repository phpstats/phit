# Model workload benchmarks

These scripts time one model fit in four implementations: the phit extension,
phit-polyfill, R, and Python statsmodels. They write two records for the
website, `site/benchmarks/ols-workload.json` and
`site/benchmarks/mixed-workload.json`. The per-model benchmarks in
`tests/benchmarks` are separate.

| Workload | Data | phit and phit-polyfill | R | statsmodels |
| --- | --- | --- | --- | --- |
| OLS | 1,000,000 rows, 10 predictors | `fit_ols()` | `lm()` | `smf.ols().fit()` |
| Gaussian mixed model | 20,000 rows in 200 groups, 3 predictors, random intercept, REML | `fit_mixed_gaussian()` | lme4 `lmer()` | `smf.mixedlm().fit(reml=True)` |

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
  check. This is not a verification; the verification pages compare the models
  with R at stated tolerances.
- The record's `code` holds the timed lines of each script verbatim. The runner
  checks that each script still contains them.

The mixed-model size is set so the polyfill finishes in well under a minute
per run. Each record states the optimizers and their convergence settings.

## Running

The runner needs the release build of the extension, a phit-polyfill checkout
with `composer install` run, R with lme4, and a Python environment with
statsmodels and pandas. It runs on macOS, where `/usr/bin/time -l` reports the
peak memory footprint.

```sh
python3 -m venv .context/bench-venv
.context/bench-venv/bin/pip install statsmodels pandas numpy
.context/bench-venv/bin/python bench/generate.py
.context/bench-venv/bin/python bench/run.py --polyfill ../phit-polyfill --polyfill-version 0.4.0
```

`generate.py` writes the data sets to `.context/bench-data` from fixed seeds.
`fit.php`, `fit.R` and `fit.py` each run one fit and print one JSON line.
`fit.php` times phit when the extension is loaded, and the polyfill when it is
not and `PHIT_POLYFILL_AUTOLOAD` names the polyfill's `vendor/autoload.php`.
