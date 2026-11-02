# Changelog

## 0.5.1

`tidy($fit, effects: 'ran_vals')` returns a mixed fit's random effects as
columns, as broom.mixed's `tidy(effects = "ran_vals")` does. The columns are
`group`, `level`, `term`, `estimate`, and `std_error`, with one row per term,
column, and group, and `confInt: true` adds normal intervals. The estimates
are the conditional modes, and the standard errors are their conditional
standard deviations, as `lme4::ranef(condVar = TRUE)` reports them. On
sleepstudy and cbpp they agree with lme4 2.0.6 to 2.7e-5. The default,
`effects: 'fixed'`, is unchanged.

## 0.5.0

`summary()` prints a model fit in the layout of R's `print(summary(fit))`, and
`tidy()` returns its coefficient table as columns, as broom's `tidy()` does.
Both take the result of any model function: OLS and generalized linear fits,
from a formula or a matrix, and Gaussian and logistic mixed fits.

The coefficient table follows `printCoefmat(digits = 4)` without significance
stars. A linear model adds the residual standard error, R-squared, and the F
statistic. A GLM adds the dispersion, the deviances, and AIC. A mixed model adds
its fit criteria, the random-effect variances, and the group counts, as lme4
prints them. `tidy(confInt: true)` adds Wald intervals, which use t with the
residual degrees of freedom when the fit reports t statistics and the normal
distribution otherwise. For mixed fits, `tidy()` gives the fixed effects only.

Every model result has a new `model` key: `linear`, `generalized_linear`,
`mixed_gaussian`, or `mixed_logistic`. `summary()` and `tidy()` choose the
printer for a fit from this key.

The fixtures in `tests/fixtures/summary` compare every printed coefficient row
with R 4.6.1 and lme4 2.0.6 on mtcars, mroz, MASS Insurance, sleepstudy, and
cbpp.

### Removed

The 0.3 names, deprecated since 0.4.0, are removed. Each replacement takes the
same arguments.

- `fit_lm()`: use `fit_ols()`.
- `fit_glm()`: use `fit_generalized_linear()`.
- `fit_lmer()`: use `fit_mixed_gaussian()`.
- `fit_glmer()`: use `fit_mixed_logistic()`, which has no `family` or `link`
  argument.
- `fit_logistic()`: use `fit_logistic_matrix()`.
- `fit_probit()`: use `fit_probit_matrix()`.
- `fit_poisson()`: use `fit_poisson_matrix()`.
- `fit_mixed_intercept()`: use `fit_mixed_gaussian_intercept_matrix()`.
- `fit_mixed()`: use `fit_mixed_gaussian_matrix()`.
- `fit_logistic_mixed()`: use `fit_mixed_logistic_matrix()`.

`fit_ols()` takes only a formula. Its matrix form, `fit_ols($x, $y)`,
deprecated since 0.4.0, is removed: call `fit_ols_matrix()`. The `offset`,
`intercept`, `x`, and `y` parameters are gone from `fit_ols()`.

## 0.4.4

The listwise-deletion warning names the function once. PHP already prefixes
the message with the function, so it now reads `Phit\fit_ols(): dropped 1 of 10
observations with missing values (listwise deletion)` instead of repeating the
name.

## 0.4.3

`fit_mixed_gaussian()` and `fit_mixed_gaussian_matrix()` scale linearly with the
number of groups for single and nested grouping terms. The random-effect columns
that share rows form independent blocks, one per group or outermost group, and
each likelihood evaluation factors the blocks separately instead of one dense
matrix. Crossed terms keep the dense path, and their results are unchanged.

The objective is the same function, so results agree with 0.4.2 to rounding. On
the lme4 fixtures, log-likelihoods differ by at most 2.1e-15 relative. Where the
different rounding changes BOBYQA's path, coefficients move by at most 2.6e-8
relative and random-effect estimates by at most 3e-6 (the near-flat four-level
nested model). Every lme4 and statsmodels comparison passes at its existing
tolerance. A random intercept on 20,000 rows took 182 ms with 200 groups and
1,473 ms with 500 in 0.4.2, and takes 10 and 12 ms now. Single and nested
designs are no longer limited to 512 random coefficients.

## 0.4.2

`read_csv()` no longer holds the whole file while it reads it. It reads the
file in blocks of 65,536 rows and converts each block to PHP values before
reading the next. When a column's type is guessed, two earlier passes count the
rows and keep the rows readr would guess from, so the types, values, missing
values and problems are unchanged. Giving every column's type skips those
passes.

On the seeded orders files read with phibble's `fromCsv()`, the data frame's
heap is unchanged (225 MiB at 2,000,000 rows, 3,585 MiB at 20,000,000). The
peak memory footprint fell from 593 to 278 MiB at 2,000,000 rows and from 5,600
to 2,220 MiB at 20,000,000. With the file in the page cache, the two extra
passes cost about half again in time: 0.43 to 0.65 s at 2,000,000 rows, and
3.9 to 6.0 s at 20,000,000. With every type given, the time is unchanged and the
footprint falls the same way.

## 0.4.1

`read_csv()` and `CsvReader` share repeated strings within a column. A value
repeated in a string column is now one PHP string, as phibble's PHP reader
holds it, instead of one string per cell. The values returned do not change. A
column stops sharing once more than half of its values are distinct (checked
from 16,384 rows on), and it shares at most 65,536 distinct values.

On a seeded 2,000,000-row orders file read with phibble's `fromCsv()`, the data
frame's PHP heap fell from 505 to 225 MiB, the same as phibble's PHP reader.
The peak memory footprint fell from 875 to 593 MiB. At 20,000,000 rows, the heap
fell from 6,389 to 3,585 MiB and the peak footprint from 8,417 to 5,599 MiB.
`bench/csv/memory.php` measures this.

## 0.4.0

phit 0.4.0 renames the model functions. Numerical results do not change: each
new name runs the same code as the old one, and the tests check that the old
and new names return identical results.

The 0.3 names still work in 0.4 as deprecated aliases. Each call raises one
`E_USER_DEPRECATED` that names the replacement, for example
`Phit\fit_lm(): Deprecated since phit 0.4.0; use Phit\fit_ols() instead`.
phit 0.5 will remove the aliases.

### Migration

| 0.3 name | 0.4 name | Input |
| --- | --- | --- |
| `fit_lm` | `fit_ols` | Formula |
| `fit_glm` | `fit_generalized_linear` | Formula |
| `fit_lmer` | `fit_mixed_gaussian` | Formula |
| `fit_glmer` | `fit_mixed_logistic` | Formula |
| `fit_ols` (matrix) | `fit_ols_matrix` | Matrix |
| `fit_logistic` | `fit_logistic_matrix` | Matrix |
| `fit_probit` | `fit_probit_matrix` | Matrix |
| `fit_poisson` | `fit_poisson_matrix` | Matrix |
| `fit_mixed` | `fit_mixed_gaussian_matrix` | Matrix |
| `fit_mixed_intercept` | `fit_mixed_gaussian_intercept_matrix` | Matrix |
| `fit_logistic_mixed` | `fit_mixed_logistic_matrix` | Matrix |

`model_matrix` and `predict` keep their names.

- **`fit_ols`** is the one name that 0.3 and 0.4 both use. In 0.4 it dispatches
  on its first argument. A string, or the named arguments `formula:` and
  `data:`, is the formula fit. An array, or the named arguments `x:` and `y:`,
  is the 0.3 matrix fit, with one deprecation pointing to `fit_ols_matrix()`.
  A call never changes meaning. In 0.5, an array passed to `fit_ols` will throw
  a `TypeError` that names `fit_ols_matrix()`. Mixing the forms, such as a
  formula with `x:`, or a formula with `offset:` or `intercept:`, throws code
  1001.
- **`fit_mixed_logistic`** has no `family` or `link` arguments, because it fits
  only the binomial family with the logit link. The deprecated `fit_glmer`
  still accepts them.
- **Messages** that name a model function now use the 0.4 name, also when the
  call came through an alias. For example, a formula fit that drops rows warns
  `Phit\fit_ols dropped 1 of 10 observations ...`.

Earlier releases are described on the
[releases page](https://github.com/phpstats/phit/releases).
