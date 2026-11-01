# Changelog

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
