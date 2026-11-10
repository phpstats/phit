# Robust covariance

This specification uses published estimating-equation formulas, public help,
and output from sandwich and ivreg APIs. No GPL package source was opened or
copied. Existing MIT formula, estimation, and fit interfaces are integration code.

## References

- Zeileis (2004), *Econometric Computing with HC and HAC Covariance Matrix
  Estimators*, JSS 11(10), <https://doi.org/10.18637/jss.v011.i10>.
- Zeileis (2006), *Object-oriented Computation of Sandwich Estimators*, JSS
  16(9), <https://doi.org/10.18637/jss.v016.i09>.
- Zeileis, Köll, Graham (2020), *Various Versatile Variances*, JSS 95(1),
  <https://doi.org/10.18637/jss.v095.i01>.
- Public help: <https://sandwich.r-forge.r-project.org/reference/vcovHC.html>,
  <https://sandwich.r-forge.r-project.org/reference/vcovCL.html>,
  <https://sandwich.r-forge.r-project.org/reference/NeweyWest.html>.
- ivreg public methods help:
  <https://zeileis.github.io/ivreg/reference/ivregMethods.html>.
- Newey and West (1987), *A Simple, Positive Semi-Definite, Heteroskedasticity
  and Autocorrelation Consistent Covariance Matrix*, Econometrica 55, 703–708.

## Geometry and weighted scores

Let D = sqrt(W) X. For IV replace D by its projection onto sqrt(W) Z, using
Householder QR. Let A = D'D. Compute the inverse Gram with column-scaled QR,
without normal equations. Row i's coefficient influence is
`sqrt(w_i) * structural_residual_i * D_i * inverse(A)`. Scaling all weights by
a common positive constant cancels. Leverage is the row squared norm of Q from
this projected weighted design. These choices match weighted public ivreg and
sandwich outputs (including HC2 and HC3); the first-stage residual is never used.

HC0 covariance sums outer products of influence rows. HC1 multiplies by
n/(n-k). HC2 divides each influence by sqrt(1-h_i); HC3 divides by 1-h_i.
HC2/HC3 at numerically unit leverage are undefined and raise 1005.

One-way cluster HC0 sums influence within each cluster, then takes outer
products and multiplies by G/(G-1). Cluster HC1 additionally multiplies by
(n-1)/(n-k). Explicitly pass type=HC1 in sandwich verification, because its
non-lm default can differ. At least two retained clusters are required. Only
HC0 and HC1 cluster corrections are supported; clustered HC2/HC3 and multiway
clustering are outside this release.

Newey–West adds lagged influence cross products and their transposes with
Bartlett weight 1-h/(L+1), for h=1..L. There is no prewhitening. Optional
adjust=true multiplies by n/(n-k). Lag is a required nonnegative integer less
than retained n; there is no automatic lag selection. Lag0 equals HC0, and
adjusted lag0 equals HC1. Rows are used in retained input order, with deleted
rows closing gaps; dates are neither sorted nor interpreted.

## Row alignment and inference

`vcov` accepts iid, HC0–HC3, [cluster => column or original-row labels,
type => HC0/HC1], or [newey_west => L, adjust => bool]. Unknown or conflicting
options raise 1001; label length mismatches raise 1002. Cluster labels may be
scalar categorical values. Missing labels participate in joint deletion with
all response, regressor, instrument, and weight columns, or raise under the
missing=raise policy. Factor levels are rebuilt on retained rows. Label keys
must preserve distinct floating-point and integer values; rounded display
strings must not merge groups. Arrays align to original rows, not retained rows.

OLS/IV coefficient p-values and intervals use t with n-k df for HC and HAC,
and G-1 for clustered covariance. `inference_df` exposes this separately from
`df_residual` (always n-k). Prediction and tidy intervals use inference_df.
`vcov`, `vcov_label`, and inference_df are present for formula OLS and IV, including
iid defaults. Matrix OLS and Gaussian GLM interfaces retain their existing API.

Residual sigma, RSS, R², coefficients, fitted values, and IV first-stage,
Wu–Hausman, and Sargan diagnostics retain their existing iid definitions.
Summaries label covariance and the coefficient-inference df. The classical
R²-based overall F is suppressed for robust fits; it is not a robust Wald test.
IV diagnostics remain explicitly labeled iid. Robust prediction intervals retain
the existing added residual sigma² convention; this is not a model for
observation-specific heteroskedastic prediction variance.

## Verification

`tests/generate-vcov-fixtures.R` records public sandwich 3.1.3 and ivreg output
on explicit inputs. It compares coefficients, covariance, SEs, t statistics,
p-values, intervals, retained rows, and prediction SEs/intervals. Cases cover
weighted/no-intercept/factor/offset OLS, weighted/no-intercept/multi-endogenous IV,
missing instruments and clusters, G=2, and heating day clusters and NW6/NW24.
Coefficient inference and prediction intervals are calculated in R using the
stated df convention. Tolerance is atol 1e-9 plus rtol 1e-7 times the reference.
PHP edge checks verify original-row labels, close-valued float and 64-bit integer
labels, covariance weight invariance, and HC/HAC identities. Diagnostics and
estimation outputs must stay unchanged for the same retained sample.
