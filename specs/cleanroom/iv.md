# Instrumental-variable estimation

This specification uses published mathematics, public documentation, and observed
reference outputs. No reference-package source was consulted or copied. The
existing MIT phit formula engine, QR utilities, and fit interfaces are integration
code and may be read for this feature. The older implementation-isolation rules
in README.md apply to the routine replacements listed there.

## References

- Sanderson and Windmeijer (2016), *A weak instrument F-test in linear IV models
  with multiple endogenous variables*, Journal of Econometrics 190, 212–221,
  equations 14–15. <https://doi.org/10.1016/j.jeconom.2015.06.004>
- ivreg public summary documentation:
  <https://zeileis.github.io/ivreg/reference/summary.ivreg.html>.
- fixest public estimation documentation:
  <https://lrberge.github.io/fixest/reference/feols.html>.
- Shea partial R² covariance-diagonal identity:
  <https://bashtage.github.io/linearmodels/iv/mathematical-formula.html>.
- Published OneSampleMR/Stata comparisons confirm the finite-sample SW degrees
  of freedom: <https://remlapmot.github.io/OneSampleMR/articles/f-statistic-comparison.html>.
  Only help and published output are references; source is excluded.

## Estimation and diagnostics

Apply the same listwise deletion to every formula part and the weights. Transform
all retained rows by sqrt(w). Let C denote exogenous columns, D endogenous
columns, X the structural design, and Z all instruments including C. Let k and L
be the numbers of X and Z columns, m the number of endogenous columns, and
q = L - cols(C). Use column-scaled Householder QR for projections and solves.
Reject order failure or numerical rank deficiency with error 1004. Require n > L.

Project H = P_Z X and solve y on H. Reconstruct fitted values from the original
structural X. Inference uses structural RSS divided by n-k and covariance
sigma² (H' H)^-1. R² = 1 - RSS/TSS, with weighted centering only when an intercept
is present. Prediction rebuilds the structural design and uses this covariance.

First-stage F compares D_j on C with D_j on Z and has df (q, n-L). Partial R²
is 1 - RSS_full/RSS_restricted. Shea partial R² is
[(X'X)^-1]_jj / [(H'H)^-1]_jj; equivalently it is the ratio of the residual sums of
squares from projecting fitted and actual endogenous columns on their respective
other columns, including C. It is null for a single endogenous variable.

For SW, regress D_j on [C, H_D,-j] to obtain the coefficients delta on the other
fitted endogenous columns. Numerator A = ||P_Z D_j - C gamma - H_D,-j delta||².
Let V = D - P_Z D. Denominator B = ||V_j - V_-j delta||².
F_SW = [A/(q-m+1)] / [B/(n-L)]. With m=1 return the first-stage F exactly.
A small numerator is a meaningful weak-instrument result. Singular conditional
regressions or undefined multi-endogenous variance return null for the SW value
and p-value, retain the df keys, and add a diagnostics warning.

Wu–Hausman compares OLS y on X with y on [X,V] using df (m,n-k-m).
Unavailable augmented regressions return nullable fields and a warning.
Sargan uses n times the uncentered explained fraction from the weighted
regression of structural residuals on Z, with chi-square df L-k. Exactly
identified fits return null.

## Observed conventions

`tests/generate-iv-fixtures.R` records ivreg 0.6-8 and fixest 0.14.2 outputs.
The fixture records actual installed versions. fixest's coefficient order puts
endogenous columns first; comparisons reorder by design column names. Its iid
small-sample correction is explicitly enabled to use n-k.

In the `no_intercept` fixture, ivreg 0.6-8 reports Sargan -269.74739212777.
The uncentered auxiliary regression gives 0.369229046986097. phit follows the
specified auxiliary-regression definition. All other retained ivreg comparisons
use its public coefficient, covariance, summary, and diagnostic outputs.

Weight rescaling must leave coefficients, covariance, first-stage diagnostics,
and structural diagnostic tests invariant. The fixture tolerance is
1e-9 + 1e-7 * abs(reference). Fixture scripts call package APIs and never inspect
function bodies. Installing reference packages in a project-local library does
not make their source an allowed reference.

In the no-exogenous-column fixture, fixest's `ivf` output differs from the
uncentered first-stage F: for e1 it reports 11.213138081675 versus
11.752431324068 from the explicit restricted/full QR regressions. This case is
compared against base R's defined uncentered regression test. Other fixest
first-stage F outputs are tested directly. ivreg's weak-instrument diagnostics
are also tested directly where exogenous columns are present.
