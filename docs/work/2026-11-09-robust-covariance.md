# Robust covariance, 0.9.0

Implemented formula OLS and IV covariance choices in native phit and the
polyfill: HC0–HC3, one-way cluster HC0/HC1, and Newey–West. All support weights.
Cluster labels align with original input rows and join shared missing-row
handling. Cluster coefficient inference uses G-1 df, exposed as inference_df;
residual df remains n-k. Prediction and tidy intervals use inference_df. Robust
summaries label the covariance and omit the classical R²-based F. IV diagnostics
retain their iid definitions. See specs/cleanroom/vcov.md for all conventions.

No GPL package source was consulted. Public sandwich/ivreg help, papers, and
fixture outputs supplied the references. Independent scientific review confirmed
the weighted IV projected scores, leverage, and corrections against public APIs.
It caught float cluster labels merging through rounded PHP strings. Label keys
now preserve close floats and 64-bit integers and normalize exactly integral
float labels consistently. Tests cover these identities.

Validation: 107 public sandwich reference cases; 134,673 numerical comparisons
per implementation, including coefficient and prediction inference. Maximum
absolute reference difference is 8.101e-13. Tolerance is 1e-9 + 1e-7*abs(reference).
Cases include G=2, offsets, factors, no intercept, missing instruments/clusters,
weighted multi-endogenous IV, heating day clusters, and Newey–West lags 6/24.
Estimation and IV diagnostics remain unchanged for the same retained sample.
Ridge, IV, and robust covariance therefore complete the three implementation
steps of the plan.

Benchmarks run only the new robust workloads: heating OLS HC3, IV HC3, IV
cluster HC1, and IV adjusted NW24. Seven repetitions use fresh processes after
an excluded warm-up and rotate execution order. Actual timings, checks, and
source hashes are in bench/vcov-results.json. R ivreg does less diagnostic work
because it has no Shea or SW output.

Required scripts/check passed on PHP 8.4 NTS. Polyfill focused tests passed
(112 tests, 7,211 assertions), followed by new edge/API/NaN checks (10 tests,
4,298 assertions) and alias-signature tests (7 tests, 30 assertions). Both
repositories pass PHPCS. The release gate initially caught an old four-argument
fit_ols signature assertion; updated native and polyfill assertions include vcov.
Generated references, 47 documentation pages, and site exports pass drift checks.

The landing-page correction was shipped first in phit 477f12a and the website
ea6b638. The website's previous content pin predated IV/ridge. Its capability
list and pinned docs were refreshed. Unrelated website SEO changes remain
untouched. Public automated HTTP verification returned 403; the focused app
render test passes. The website receives a final 0.9.0 content refresh after
the paired release publishes.
