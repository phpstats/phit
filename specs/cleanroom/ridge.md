# Gaussian ridge and cross-validation

This implementation follows public mathematics, help pages, and observed API
outputs. No glmnet, MASS, lmridge, or R source was opened or copied. The existing
MIT formula engine, QR utilities, and fit interfaces are integration code.

## References

- Friedman, Hastie, Tibshirani (2010), *Regularization Paths for Generalized
  Linear Models via Coordinate Descent*, JSS 33(1),
  <https://doi.org/10.18637/jss.v033.i01>.
- glmnet public vignette: <https://glmnet.stanford.edu/articles/glmnet.html>.
- Public help: <https://glmnet.stanford.edu/reference/glmnet.html> and
  <https://glmnet.stanford.edu/reference/cv.glmnet.html>.
- MASS public help:
  <https://stat.ethz.ch/R-manual/R-devel/library/MASS/html/lm.ridge.html>.
- lmridge public help:
  <https://cran.r-project.org/web/packages/lmridge/lmridge.pdf>.
  Only its documented ridge estimator and public output are used here.

## Scaling observed on explicit lambda inputs

`tests/generate-ridge-fixtures.R` calls only public APIs and records glmnet 5.1,
MASS, and lmridge versions. `tests/fixtures/ridge-r.json` preserves inputs and
outputs. For q = w/sum(w), let sy be weighted centered response SD with an
intercept, or weighted response RMS without one. The quadratic penalty in the
original response units is **lambda/sy**, rather than literal lambda in the
simplified objective in the original project plan. `lambda_effective` and
`response_scale` expose these quantities. This response scaling is required to
reproduce explicit-lambda Gaussian glmnet output even with standardize=false.

With an intercept, predictors are centered by weighted means. Without one, they
are not centered. When standardize=true, both cases divide by the **centered**
weighted predictor SD using divisor sum(w). The no_intercept fixture separates
this convention from uncentered RMS scaling. Constant predictors are excluded
and have coefficient zero. Predictors return to original units afterward.

Penalty factors are nonnegative and normalized to sum to p predictor columns,
including zero-penalty columns in p. The intercept has penalty zero. If all
factors are zero, there is no penalty. Patterns use literal design names plus
`*` and `?`; unmatched patterns raise 1001. Bounds apply to original-scale
predictors, exclude the intercept, and transform with predictor scales.
Overlapping patterns apply in insertion order, with later matches winning.

For standardized design Z, G = Z' diag(q) Z and A = G + (lambda/sy) P.
Solve the augmented least-squares system with QR; bounded fits use projected
coordinate descent with a KKT convergence check. Equality-fixed coordinates are
removed from the initial solve. Lambda zero needs an identifiable remaining
unpenalized design (1004 on rank deficiency). Positive penalties support
collinear predictors.

## Conditional uncertainty

On the free coefficient face, df = trace(A^-1 G). Weights are normalized to sum
to n for RSS; sigma² = RSS/(n-df). Covariance is sigma²/n times
A^-1 G A^-1, transformed back to original coefficient units. At-bound rows and
columns, SEs and confidence intervals are null. Free-coefficient uncertainty
conditions on the selected active face. Prediction uncertainty is null for a
row with a nonzero contribution from an at-bound coefficient. Confidence
intervals use a t reference with n-df degrees of freedom. They hold lambda fixed,
ignore shrinkage bias and model selection, and are **not coverage-valid**.
Prediction intervals are unsupported (1007).

For unweighted, standardized, intercept, uniform-penalty, unbounded fits:
`lambda_lm_ridge = n*lambda/sy`. This reproduces MASS coefficients. Other cases
report null; there is no single general scalar conversion.

lmridge with scaling='sc' uses L2 predictor norms, so K=lambda/sy. Its public
coefficient accessor rounds to five decimals; the returned coefficient and scale
fields preserve precision. Its public hat matrix is rounded to five decimals.
Observed residual variance uses n-2*trace(H)+trace(H'H), which differs from phit's
n-df. The fixture compares slopes and conditional covariance **per sigma²**,
with the rounding and variance denominator recorded explicitly. phit's variance
estimate is independently computed in R from the stated n-df definition.

## Weighted CV observed on unequal folds

Each training subset recomputes all predictor and response scales. Held-out fold
k has Wk=sum(w_i) and Ek=sum(w_i*error_i²)/Wk. With K nonempty folds:

- cvm = sum(Wk*Ek)/sum(Wk).
- cvsd = sqrt((sum(Wk*(Ek-cvm)²)/sum(Wk))/(K-1)).

The unequal_folds_and_weights fixture separates weighted from unweighted fold
averaging. day_folds_with_age_weights groups hourly rows by day and combines
these fold ids with age-decaying weights. Missing rows are removed jointly with
fold ids. If floor(n/K)<3, glmnet switches to ungrouped errors: apply the same
weighted mean/variance calculation to individual squared errors, denominator
n-1. small_folds_ungrouped pins down that fallback.

Explicit paths must be finite, nonnegative, strictly decreasing. Indices are
zero-based. lambda_min minimizes cvm; lambda_1se is the largest lambda with
cvm <= cvm[min]+cvsd[min]. The returned fit uses lambda_min on all retained rows.
Explicit integer fold labels override nfolds and seed and require at least three
nonempty folds. Generated folds use a balanced seeded permutation of retained
rows. At least one positive-weight row must remain in each training/test fold.

The default is phit's own response-scale path: 100 log-spaced values from
100*sy down to .01*sy, inclusive. It is deterministic for the full retained data
and does not claim to reproduce glmnet's default path. Explicit paths are used
for reference comparisons.

## Verification tolerance

Use atol=1e-7 and rtol=2e-6 for glmnet comparisons. The heating design at the
planned thresh=1e-14 differed by a few millionths; reference calls use
thresh=1e-18 and maxit=1,000,000 to reduce stopping error. The verification record
preserves actual values, maximum absolute errors, and maximum fractions of the
allowed tolerance. MASS conversions and lmridge covariance are tested separately.
