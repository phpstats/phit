# Plan: 2SLS, ridge, and robust covariance

Status: steps 1 and 2 shipped in both packages as 0.7.0 and 0.8.0.
Step 3 ships robust covariance in both packages as 0.9.0. Cluster coefficient inference uses G-1 df;
residual df remains n-k. Under-identified IV models use 1004.
Ridge includes conditional SEs by default; its default CV path is 100
log-spaced values from 100*response_scale to .01*response_scale.
Gaussian glmnet uses lambda/response_scale in the original response units;
see specs/cleanroom/ridge.md for the observed correction to the simplified
objective below. Implementation conventions and observed reference behavior are recorded in
`specs/cleanroom/vcov.md`.


phit gains three things, delivered in this order:

1. `fit_iv()`: two-stage least squares with first-stage diagnostics.
2. `fit_ridge()` and `cv_ridge()`: ridge regression with unpenalized terms, standardization, bounds, and
   cross-validation over fold ids that the caller supplies.
3. `vcov:`: heteroskedasticity-robust, clustered, and Newey–West covariance on `fit_ols()` and `fit_iv()`.

Each step ships in phit (Rust core plus extension) and in phit-polyfill (`~/Projects/phit-polyfill`) at the same time,
with R verification records, and is released as its own version.

## Who asked and why

The life app (`~/Projects/life`, T3 thread "Optimize Whole-Home Heating Setpoints") fits one model per room every night
from hourly Home Assistant history:

```
dT_next_hour ~ 0 + (outdoor - indoor) + (neighbours - indoor) + hp_kwh + hp_kwh:outdoor
             + floor_call_lag0 + ... + floor_call_lag6 + factor(hour)
```

That is about 35 coefficients and 1,000 to 3,000 rows per room. The app runs PHP 8.4 NTS. Thermostats close the loop,
so OLS biases the heating coefficients, and 2SLS fixes that. The instruments are Home Assistant's clock-driven
schedule changes. The lag terms are collinear and come out negative, which is physically impossible. Ridge with
non-negative bounds fixes that. Until each release lands, the life app uses plain-OLS stand-ins behind an interface:
augmented rows for ridge, and a manual 2SLS with a σ² correction. The life app accepted the signatures and order below.

### Acceptance criteria from the life app

- **The Sanderson–Windmeijer F must always be present.** `fit_iv()` reports it for every endogenous variable whenever
  there are two or more. Their schedule instruments are often weak for some variables in some rooms, and they gate on
  this number. A weak or near-collinear first stage must still produce a finite value or a documented `null` with a
  warning. It must never be silently missing.
- **Fold ids with weights.** The `cv_ridge()` verification fixtures cover `foldid` combined with `weights`: folds by
  day, and weights that decay with age.

## Clean-room rule

phit is MIT-licensed. R and the reference packages are GPL: glmnet, AER, ivreg, fixest, sandwich, MASS, and lmridge.
Follow the process in `specs/cleanroom/README.md`:

- Do not open, search, or recall the source of those packages.
- Work from these sources:
  - published papers;
  - the packages' help pages and vignettes;
  - JSS articles, such as Friedman, Hastie and Tibshirani (2010) for glmnet, and Zeileis (2004, 2006) for sandwich;
  - outputs observed on fixture inputs.
- Where a convention can only be pinned down from output, such as how cv.glmnet weights fold errors in `cvm` and
  `cvsd`, record it in a short spec under `specs/cleanroom/` with the fixture that shows it.
- Add the references to `THIRD_PARTY_NOTICES.md` only if behavior is followed, not code.

## Where things go

The plan follows how OLS is built today:

| Piece | Location |
| --- | --- |
| Estimator | `crates/stats-core/src/` (new `iv.rs`, `ridge.rs`, `vcov.rs`) |
| PHP binding and fit classes | `crates/php-extension/src/fit.rs`, `printer.rs` |
| Stubs (the documented API) | `stubs/phit.php`, checked by `scripts/php-stubs.php --check` |
| Fixture generators | `tests/generate-iv-fixtures.R`, `tests/generate-ridge-fixtures.R`, `tests/generate-vcov-fixtures.R` |
| PHP tests | `tests/iv.php`, `tests/ridge.php`, `tests/vcov.php`, added to the suite loop in `scripts/check` |
| Verification records | `docs/verifications/*.R` and `compare_*.py`, `site/verifications/{iv,ridge,vcov}.json` |
| Docs | `docs/models.qmd`, `docs/formulas.qmd` (the IV formula), `docs/reference-models.qmd`, `docs/r-compatibility.qmd` |
| Polyfill | `~/Projects/phit-polyfill/src/` (new `Iv.php`, `Ridge.php`, `Vcov.php`), same fixtures under `tests/fixtures` |

## 1. `fit_iv()`

```php
function fit_iv(
    string $formula,          // 'y ~ exog | endog ~ instruments', as fixest
    mixed $data,
    string $missing = 'drop',
    mixed $weights = null,    // array or column name, as ivreg(weights =)
    string|array $vcov = 'iid',   // 'iid' only in step 1; step 3 adds the rest
): IvFit;
```

### Formula

- `'y ~ x1 + factor(h) | e1 + e2 ~ z1 + z2'`. The first part is the outcome and the exogenous regressors, which act as
  their own instruments. After the top-level `|` come the endogenous regressors, then `~`, then the excluded
  instruments.
- `~ 0 + …` and `- 1`, `a:b`, `factor()`, and the existing transforms work in every part.
- Interactions that contain an endogenous variable, such as `hp_kwh:outdoor`, are endogenous. The caller lists them
  after the `|`, written as `hp_kwh:outdoor`.
- The parser already treats `|` as an operator for random-effect bars (`crates/stats-core/src/formula/parse.rs`).
  - A top-level `|` outside parentheses splits an IV formula. Bars inside `( … | g)` stay random effects, which
    `fit_iv` rejects with 1007.
  - `fit_ols()` and the mixed functions keep rejecting a top-level IV split.
- `model_matrix()` is not changed. A helper returns the three designs (X exogenous, X endogenous, Z excluded) with
  the same missing-row handling across all of them. A row is dropped if any part uses a missing value.

### Estimation

- Use a QR-based 2SLS: project X = [X_exog, X_endog] onto Z = [X_exog, Z_excl], then solve. Weighted estimation
  multiplies each row by √w first.
- Throw (1004 or a new code) when the order condition fails (fewer excluded instruments than endogenous variables)
  or Z is rank-deficient. This matches how `fit_ols()` treats rank deficiency.
- Standard errors use the structural residuals y − Xβ̂, with σ² = RSS / (n − k), as `AER::ivreg` does. The naive
  second-stage residuals are never used.

### `IvFit`

`IvFit` is a new class that implements `Fit`. It has every `LinearFit` key: coefficients, covariance, std_errors,
p_values, t_statistics, conf_int, named, fitted, residuals (structural), sigma, df_residual, r_squared
(as ivreg defines it), columns, terms, assign, intercept, predvars, and the row bookkeeping. It adds these keys:

```
endogenous:  list<string>                     // design columns treated as endogenous
instruments: list<string>                     // excluded instrument columns
first_stage: array<string, array{             // keyed by endogenous column
    f_statistic: float, df1: int, df2: int, p_value: float,   // F test of the excluded instruments
    partial_r_squared: float,                                  // R² of excluded instruments, partialling out exogenous
    shea_partial_r_squared: float|null,                        // only with 2+ endogenous variables
    sw_f_statistic: float|null, sw_df1: int, sw_df2: int, sw_p_value: float|null,
    coefficients: array<string,float>,
}>
diagnostics: … wu_hausman: {statistic, df1, df2, p_value},
             sargan: {statistic, df, p_value} | null          // null when exactly identified
```

- Compute the Sanderson–Windmeijer F from Sanderson and Windmeijer (2016, *Journal of Econometrics* 190(2)).
  - With one endogenous variable, it equals the first-stage F. The tests assert this.
  - With two or more, a near-singular conditional regression gives `null` and a warning in `diagnostics.warnings`.
    The value is never silently left out.
- Wu–Hausman is the regression-based test that ivreg's `summary(diagnostics = TRUE)` reports. Sargan is n·R² from
  regressing the structural residuals on all instruments, with df = L − K.
- `predict()` uses the structural equation. `seFit` and `interval: 'confidence'` use the IV covariance. A prediction
  interval adds σ².
- `summary()` prints like `summary(ivreg, diagnostics = TRUE)`, with the coefficient table and the diagnostics block,
  plus the SW F rows. `tidy()` works as it does for OLS.

### Verification

`tests/generate-iv-fixtures.R` covers these cases:

- one and two endogenous variables;
- exactly identified and over-identified;
- with and without an intercept, with `factor()` and with interactions;
- with weights;
- with missing rows;
- a weak-instrument case;
- a simulated heating-shaped case with about 35 columns, `factor(hour)`, and schedule-indicator instruments.

The references are:

- `AER::ivreg` (and the newer `ivreg` package): coefficients, covariance, SEs, the weak-instrument F,
  Wu–Hausman, and Sargan.
- `fixest::feols(y ~ x | 0 | e ~ z)`: coefficients, SEs, and the first-stage F from `fitstat(…, "ivf")`.
- The SW F, computed in the generator from the paper's definition with `lm()`. No reference package exposes it. Use
  Stata's `ivreg2` output for one case if it is available.

The tolerance is the OLS model rule: atol 1e-9, rtol 1e-7.

## 2. `fit_ridge()` and `cv_ridge()`

```php
function fit_ridge(
    string $formula,
    mixed $data,
    float $lambda,
    array $penaltyFactor = [],   // column name or glob => factor ≥ 0; 0 leaves the column unpenalized
    bool $standardize = true,
    array $bounds = [],          // column name or glob => [lower|null, upper|null]
    string $missing = 'drop',
    mixed $weights = null,
): RidgeFit;

function cv_ridge(
    string $formula,
    mixed $data,
    ?array $lambda = null,
    ?array $foldid = null,       // one fold id per input row; takes precedence over nfolds
    int $nfolds = 10,
    int $seed = 0,
    array $penaltyFactor = [],
    bool $standardize = true,
    array $bounds = [],
    string $missing = 'drop',
    mixed $weights = null,
): array; // {lambda, cvm, cvsd, cvup, cvlo, lambda_min, lambda_1se, index_min, index_1se, fit: RidgeFit at lambda_min}
```

### Objective

The objective is glmnet's with alpha = 0:

```
(1 / (2 Σw)) Σ w_i (y_i − β0 − x_iᵀβ)²  +  (λ / 2) Σ_j pf_j β_j²
```

- Weights are rescaled to sum to n, and penalty factors to sum to the number of columns, as the glmnet help page
  states.
- With `standardize`, columns are scaled by their weighted standard deviation, using the 1/n divisor. Coefficients
  are returned on the original scale.
- The intercept is never penalized. With `~ 0`, there is no intercept and columns are not centered.
- Confirm from fixtures how glmnet treats `standardize` without an intercept, and write the result in a spec.

### Other rules

- Glob keys in `penaltyFactor` and `bounds` match design column names, such as `floor_call_lag*` and
  `factor(hour)*`. A key that matches nothing throws 1001, so typos fail loudly.
- Without bounds, the fit is closed form: solve the augmented system with QR. With bounds, use coordinate descent or
  an active-set method to a tight tolerance. Bounds use the original coefficient scale, as glmnet's
  `lower.limits`/`upper.limits` do. `lambda = 0` with bounds gives bounded least squares.

### `RidgeFit`

`RidgeFit` is a new `Fit` class. Its keys:

- coefficients, named, fitted, residuals, columns, terms, assign, intercept, predvars, and the row bookkeeping;
- `lambda`, `penalty_factor`, `standardize`, `bounds`, and `at_bound` (list<bool>);
- `df` (effective degrees of freedom, the trace of the hat matrix), `sigma` (RSS / (n − df)), `rss`, and `r_squared`;
- `lambda_lm_ridge`: the same fit in `MASS::lm.ridge`'s scaling. Establish the conversion from the help pages and
  fixtures, then document it in `docs/models.qmd`.
- `covariance`, `std_errors`, and `conf_int` use σ̂²(XᵀWX + λP)⁻¹XᵀWX(XᵀWX + λP)⁻¹, conditional on λ, as
  `lmridge` reports. The docs and `summary()` say these are biased and not coverage-valid. They are `null` for a
  coefficient at a bound.
- `predict()` returns fitted values. It supports `seFit` from that covariance, but not `interval: 'prediction'`.

### `cv_ridge()`

- `lambda: null` uses phit's own default path (100 values, log-spaced). Verification passes explicit sequences, because
  glmnet's default ridge path is a convention, not a documented formula.
- Every fold fits the full lambda sequence. `cvm` is mean squared error. The fold weighting of `cvm` and `cvsd` with
  `weights` must match `cv.glmnet(…, foldid =, weights =, grouped = TRUE)` and is pinned down by fixtures.
- `lambda_1se` is the largest λ with `cvm ≤ cvm[min] + cvsd[min]`.
- With `foldid`, the results are deterministic. `seed` only drives `nfolds` assignment, through `permutation_rng`.

### Verification

`tests/generate-ridge-fixtures.R` checks against `glmnet(alpha = 0, thresh = 1e-14)` and `cv.glmnet`. The cases:

- standardize on and off;
- intercept and `~ 0`;
- `penalty.factor` with zeros;
- weights;
- `lower.limits = 0` on some columns;
- **`foldid` combined with weights**: day folds over hourly rows, with weights that decay with age (from the life
  app's acceptance criteria);
- the simulated heating-shaped case.

Also check `MASS::lm.ridge` (coefficients and the λ conversion) and `lmridge` (SEs). The glmnet tolerance may need to
be looser than the OLS rule because glmnet stops at its own threshold. Record the tolerance used and the largest
error seen.

## 3. `vcov:` on `fit_ols()` and `fit_iv()`

```php
vcov: 'iid' | 'HC0' | 'HC1' | 'HC2' | 'HC3'
    | ['cluster' => 'day']                      // column name or array of ids aligned with the input rows
    | ['cluster' => 'day', 'type' => 'HC0']
    | ['newey_west' => 6]                       // lag; Bartlett kernel; rows must be in time order
    | ['newey_west' => 6, 'adjust' => true]
```

### Behavior

- The choice replaces `covariance` and everything derived from it: std_errors, t, p, conf_int, `predict` seFit and
  intervals. The fit records the choice in a `vcov` key, and `summary()` names it.
- HC0–HC3 match `sandwich::vcovHC`. A cluster matches `sandwich::vcovCL(type = "HC1")` by default, with
  G/(G − 1) and (n − 1)/(n − k). Newey–West matches `sandwich::NeweyWest(lag = L, prewhite = FALSE)`.
- Newey–West uses row order and ignores gaps from dropped rows. The docs state this. fixest's defaults are
  documented as differences, not matched.
- For `fit_iv`, the sandwich uses the 2SLS bread and the structural residuals, as `sandwich` does on `ivreg` objects.
- Cluster p-values and intervals use t with G − 1 df, as fixest does, or the residual df, as sandwich with `coeftest`
  does. Pick one, document it, and test it.

### Verification

`tests/generate-vcov-fixtures.R` checks against `sandwich` on `lm` and `ivreg` fits, with weights, factors, and
`~ 0`. The heating-shaped case has clusters by day and Newey–West with lag 6 and lag 24.

## Not in scope

- Lasso and elastic net. `alpha` stays 0, although the coordinate-descent code would allow it later.
- GMM or LIML.
- Driscoll–Kraay.
- Bounds on `fit_ols()`. Use `fit_ridge(lambda: 0, bounds: …)` instead.
- Robust covariance for GLMs and mixed models.

## Resolved decisions

- Under-identified IV models reuse error code 1004.
- Cluster coefficient inference uses t with G-1 df; residual df remains n-k.
- Ridge includes conditional standard errors by default, with bias and coverage caveats and null uncertainty at bounds.
- The default CV path has 100 log-spaced values from 100*response_scale to .01*response_scale. It is phit policy; explicit paths reproduce glmnet.
