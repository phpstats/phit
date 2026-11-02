# summary() and tidy() specification

This is the contract that `Phit\summary()` and `Phit\tidy()` implement in
`crates/stats-core/src/summary.rs` and `crates/php-extension/src/printer.rs`. A
second implementation, such as phit-polyfill, must produce the same bytes for
every case in `summary.json`.

## Signatures

```php
function summary(array $fit): string {}
function tidy(array $fit, bool $confInt = false, float $level = 0.95): array {}
```

## Rounding primitive

Every number goes through correctly rounded decimal conversion of the exact
binary double, as C's `printf("%.*f")` and `printf("%.*e")` do on glibc and
macOS. Rust's `format!("{:.*}")` and `format!("{:.*e}")` are the same
conversion. PHP's `sprintf('%.*F')` and `sprintf('%.*e')` agree for the values
here. PHP's `round()` and `number_format()` pre-round and do not agree.

- `sig(x, d)`: the `d` significant digits of |x| and the decimal exponent, taken
  from `%.{d-1}e`.
- `round(x, d)`: R's `round(x, d)`, implemented as parsing `%.{d}f` back to a double.

## format_real(xs, digits)

This is R's `format(xs, digits = digits)`, where one layout and one width
apply to the whole vector.

For each finite x (zero gives `neg = false`, `kpower = 0`, `nsig = 1`, and `widens = false`):

- `kpower` is the exponent of `sig(x, digits)`.
- `nsig` is `digits` less the trailing zeros of the mantissa, and at least 1.
- `widens` is true when `kpower` is from 1 to 22 and `|x|`, printed with
  `max(0, digits - 1 - kpower)` decimals and parsed back, is below `10^kpower`.
  For example, 99996 at 4 digits prints as `99996`, while 99999.6 prints as
  `1e+05`.
- `left = kpower + 1 - widens`.
- `sleft = neg + (left <= 0 ? 1 : left)`.

Over the finite values:

- `rgt = max(0, max(nsig - left))`.
- `mxsl = max(1, max(sleft))`.
- `mxns = max(nsig)`.
- `mxe` and `mne` are the largest and smallest `kpower`.
- The fixed width is `mxsl + rgt + (rgt > 0)`.
- The scientific width is `neg + (mxns > 1) + (mxns - 1) + 4 + e`. Here `neg`
  is true if any value is negative, and `e` is 2 when `mxe >= 100` or
  `mne <= -99`, else 1.
- Scientific notation is used only when the fixed width is greater than the
  scientific width.

Each value is printed as follows:

- Fixed notation is `%.{rgt}f`.
- Scientific notation is `%.{mxns-1}e`, with the exponent rewritten as a sign
  and at least two digits (`e-16`, `e+05`).
- `-0.0` prints as `0` with the fixed decimals.
- NaN, INF, and -INF print as `NaN`, `Inf`, and `-Inf`.

All values are right-aligned to the widest text.

## format_g(x, digits)

This is C's `%.{digits}g`, as R's `formatC(x, digits = digits)` gives it.

- Zero prints as `0`, and NaN and infinities print as above.
- With `e` as the exponent of `sig(x, digits)`, scientific notation is used
  when `e < -4` or `e >= digits`. Otherwise the value is `%.{digits-1-e}f`.
- Trailing zeros and a trailing point are removed from the mantissa or the
  fixed text. The exponent has the R form.

## format_pval(ps, digits)

This is R's `format.pval(ps, digits)` with `eps` equal to the machine epsilon
(2.220446e-16).

- NaN prints as `NA`.
- Values at or above `eps` are split into two groups by
  `expo = floor(log10(p))`. A value is fixed when `expo >= -3`, or when
  `expo == -4` and `digits > 1`. Each group is formatted jointly with
  `format_real(group, digits)`.
- Values below `eps` print as `<` plus the separator plus
  `format_real([eps], d)`, where `d = max(1, digits - 2)`.
  - When some values are at or above `eps`, let `nc` be the widest text among
    them. If `d > 1` and `d + 6 > nc`, then `d = max(1, nc - 7)`. The separator
    is empty when `d == 1` and `nc <= 6`, and a space otherwise.
  - When no value is at or above `eps`, the separator is empty when `d == 1`,
    and a space otherwise.

The table then trims each p-value. The result is `<2e-16` or `< 2e-16`,
following R.

## Coefficient table

This is R's `printCoefmat(digits = 4, signif.stars = FALSE)`.

- Estimates and standard errors are formatted together. Let
  `digmin = 1 + floor(log10(min |v|))` over the finite, nonzero values of both
  columns, or 1 when there are none. Each value is rounded with
  `round(v, max(1, 4 - digmin))`. All 2p rounded values are then formatted with
  one `format_real(·, 4)` call, and the first p texts are the estimates.
- Each statistic is formatted with `format_real(round(s, 3) for all s, 4)`.
- P-values are formatted with `format_pval(ps, 3)`.
- Cells are trimmed. A NaN estimate, standard error, or statistic prints as `NA`.
- The header is `""`, `Estimate`, `Std. Error`, `t value` or `z value`, and
  `Pr(>|t|)` or `Pr(>|z|)`.

## Tables

Every table works the same way.

- Each column's width is the widest of its header and its cells.
- The first column is left-aligned. Columns listed as left-aligned are padded
  on the right, and all other columns on the left.
- Cells are joined by one space. Trailing spaces are removed from every line.
  Each line ends in `\n`.

## Printers

`summary()` and `tidy()` choose a printer from the fit's `model` key, which
every model function sets. They never infer the kind from the other keys. A
missing `model` key, an unknown value, or a missing key that the printer
needs throws.

| `model` | Set by | Printer |
| --- | --- | --- |
| `linear` | `fit_ols`, `fit_ols_matrix` | `Linear` |
| `generalized_linear` | `fit_generalized_linear`, `fit_logistic_matrix`, `fit_probit_matrix`, `fit_poisson_matrix` | `GeneralizedLinear` |
| `mixed_gaussian` | `fit_mixed_gaussian`, `fit_mixed_gaussian_matrix`, `fit_mixed_gaussian_intercept_matrix` | `MixedGaussian` |
| `mixed_logistic` | `fit_mixed_logistic`, `fit_mixed_logistic_matrix` | `MixedLogistic` |

Each printer implements one contract (`Printer` in
`crates/stats-core/src/summary.rs`):

- `heading()`: the method line, then `Family:` and `Formula:` when known.
- `before_coefficients()`: fit statistics printed before the table. This is
  empty except for mixed models.
- `coefficients_title()`: `Coefficients` or `Fixed effects`.
- `coefficients()`: the table's data. `tidy()` reads the same data.
- `after_coefficients()`: fit statistics printed after the table.
- `interval_df()`: the t degrees of freedom for `tidy()` intervals, or none
  for normal intervals.

`render()` composes the output as `heading`, then `before_coefficients`, then
`\n{title}:\n`, then the coefficient table, then `after_coefficients`. The
registry is `PRINTERS` in `crates/php-extension/src/printer.rs`.

A new model kind needs one printer, one registry entry, and a fixture case.
`tests/summary.php` fails when any `Phit\fit_*` function has no case.

The following keys are read for every kind:

- `coefficients`, `std_errors`, `p_values`, and either `t_statistics` (the statistic is t) or
  `z_statistics` (the statistic is z). A null element reads as NaN.
- `columns` gives the term names. Without it, the names are `x1`, `x2`, and so on.
- `df_residual` and `n_observations` are integers. Integral floats are accepted.
- `formula` is optional.
- `intercept` (bool) is optional.

Each kind also reads its own keys:

- **Linear:** `sigma`, `r_squared`, and `adj_r_squared`. The F line appears only
  when `intercept` exists, with numerator df `p - intercept` greater than 0.
  `F = (R² / num) / ((1 - R²) / df_residual)`, and p is the upper tail of
  F(num, df_residual).
- **GLM:** `null_deviance`, `deviance`, and `aic`. The dispersion is
  `residual_variance` when the statistic is t, and 1 otherwise. The null df is
  `n_observations - intercept`, and it is printed only when `intercept` exists.
- **Mixed Gaussian and mixed logistic:** `log_likelihood`, plus `aic` and
  `bic` (each optional), and `singular` or else `boundary` (false when both
  are absent).
  - Mixed Gaussian also reads `method` (REML when it equals `"REML"`) and
    `residual_variance`.
  - Mixed logistic also reads `integration` (required) and
    `quadrature_points`.
  - Random terms come from `random`, a list of arrays. Each term has `group`
    (default `g{i}`, 1-based), `columns` (default `z{j}`), `covariance`, and
    `correlated` (default true). The group count is the size of `group_labels`.
  - When `random` is absent, or the first term has no `covariance`, the model
    has one term: `group` (from the term, else `"group"`) with an
    `(Intercept)` column, variance `random_intercept_variance`, and the
    `n_groups` groups.
  - An `integration` value of `"laplace"` prints as `Laplace approximation`.
    Any other value prints as
    `adaptive Gauss-Hermite quadrature, {quadrature_points} points`.

## Layout

Every line ends in `\n`, so the text ends with exactly one newline. In the
examples below, `{}` marks a substitution.

The heading comes first, followed by `Family: {family} ({link})` and
`Formula: {formula}` when they are present. A GLM prints the family line when
the fit has both `family` and `link`. Mixed logistic always prints
`Family: binomial (logit)`.

- Linear: `Linear model fit by least squares`.
- GLM: `Generalized linear model fit by maximum likelihood`.
- Gaussian mixed: `Linear mixed model fit by REML` or `Linear mixed model fit by maximum likelihood`.
- Binomial mixed: `Generalized linear mixed model fit by maximum likelihood ({integration text})`.

### Linear

```
\nCoefficients:\n{table}
\nResidual standard error: {format_real([sigma], 4)} on {df_residual} degrees of freedom\n
Multiple R-squared: {format_g(r2, 4)}, Adjusted R-squared: {format_g(adj, 4)}\n
F-statistic: {format_g(F, 4)} on {num} and {df_residual} DF, p-value: {format_pval([p], 4)}\n   (optional)
```

### GLM

```
\nCoefficients:\n{table}
\n(Dispersion parameter for {family} family taken to be {format_real([d], 7)})\n\n
    Null deviance: {D0} on {dfnull} degrees of freedom\n
Residual deviance: {D} on {df_residual} degrees of freedom\n
AIC: {format_real([aic], 5)}\n
```

- `D0` and `D` come from one `format_real([null, resid], 5)` call, so they
  share a width.
- With a null df, both dfs come from one `format_real([dfnull, df_residual], 15)`
  call, so they share a width.
- Without a null df (a matrix fit), the first line is `    Null deviance: {D0}\n`,
  and the residual df is printed as a plain integer.
- Without a family, the line is `(Dispersion parameter taken to be ...)`.

### Mixed

REML fits print `\nREML criterion at convergence: {format_real([-2 logLik], 5)}\n`.

ML fits print a two-line table preceded by a blank line. Its columns are AIC
and BIC (each only when present), logLik, deviance (`-2 logLik`), and
df.resid. Each value is `%.1f`, and df.resid is an integer. Each column is
right-aligned to the wider of its name and its value, and the columns are
joined by one space.

Next comes `\nRandom effects:\n`, followed by a table with these columns:

- Groups (left-aligned), filled on a term's first row only.
- Name (left-aligned).
- Variance and Std.Dev. Every variance in the table is formatted in one
  `format_real(·, 4)` call, and every standard deviation (`sqrt(variance)`) in
  another. Both are trimmed.
- One correlation column for each correlated term column beyond the first:
  the widest term less 1. The first of these columns is headed `Corr`, and the
  rest have empty headers. Row j fills cells 0 to j-1 with
  `cov[j][m] / sqrt(cov[j][j] cov[m][m])` as `%.2f`, with -0 printed as `0.00`.
- A Gaussian model adds a final row: `Residual`, an empty name, and the residual variance.

Then:

```
Number of obs: {n_observations}, groups: {group}, {n}[; {group}, {n}]...\n
\nFixed effects:\n{table}
\nThe fit is singular: a random-effect variance or correlation is at its boundary.\n   (only when singular)
```

Each distinct group is listed once, in order of first appearance.

## tidy()

- `level` must satisfy `0 < level < 1`. Otherwise it throws code 1001 with
  the message `level must be in (0, 1)`. The level is checked before the fit.
- Columns, in order: `term` (the names above), `estimate`, `std_error`,
  `statistic`, and `p_value`. `term` is a list of strings and the rest are lists of floats, and a null element in
  the fit becomes NaN.
- With `conf_int`, the columns `conf_low` and `conf_high` are added.
  - The interval is `estimate ∓ q · std_error`, with `q` the
    `(1 + level) / 2` quantile.
  - `q` comes from t with `df_residual` degrees of freedom when the fit has
    `t_statistics`, and from the standard normal otherwise.
  - For OLS this matches R's `confint(lm)`. For GLMs it gives Wald intervals,
    not profile intervals.
- Mixed fits give fixed effects only, as broom.mixed's `effects = "fixed"`
  does. Variance components appear only in `summary()`.

## Errors

Every error is an `\Exception` with code 1001.

- `fit must be a result of a Phit model function: {detail}`, where `{detail}`
  is one of the following:
  - `unknown model '{model}'`
  - `missing {key}`
  - `{key} must be numbers`
  - `coefficient lists differ in length`
  - `random terms must be arrays`
  - `a random term's covariance must be square`
  - `a random term's columns and covariance differ in size`
- `level must be in (0, 1)`.

## Fixtures

`summary.json` holds:

- `R` and `lme4`, the versions that printed `r_rows`.
- `data`, keyed by dataset name, holding the columns as `read_csv()` returns
  them. cbpp's `period` is stored as strings, so it is a factor.
- `cases`, a list. Each case has:
  - `name`.
  - `function`, a phit function name without the namespace.
  - `args`, the named arguments. When `data` is a string, it names the dataset.
  - `summary`, the exact expected text.
  - `tidy`, the expected `tidy($fit, confInt: true)`.
  - `r_rows`, R's printed coefficient rows.

`tests/summary.php` checks the summary text byte for byte. It checks the tidy
terms exactly and the tidy numbers to a relative difference of 1e-5, because
the iterative fits differ between platforms: by about 1e-14 for GLMs and by up
to 7e-7 for the mixed logistic fits on Linux. It also checks that each number in each coefficient row matches
R's row, allowing one unit in the last printed place. Random-effect variances
are not compared with R, because lme4's optimizer stops at a slightly
different point.
