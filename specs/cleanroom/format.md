# R's printed number format

`Phit\summary()` prints numbers the way R prints them. This specification describes that output. It draws on R's
help pages (`?format`, `?formatC`, `?format.pval`, `?printCoefmat`, `?options` for `scipen`) and on R's output for
the inputs in `data/`.

## Interface

```rust
/// Significant digits of a coefficient table: R's max(3, getOption("digits") - 3) = 4.
pub const DIGITS: usize = 4;

/// R's format(xs, digits = digits): every element in one common notation and padded on the left to one width.
pub fn format_real(xs: &[f64], digits: usize) -> Vec<String>;

/// R's round(x, decimals).
pub fn round(x: f64, decimals: usize) -> f64;

/// R's formatC(x, digits = digits), which is C's printf("%.{digits}g").
pub fn format_g(x: f64, digits: usize) -> String;

/// R's format.pval(pvalues, digits = digits) with eps = f64::EPSILON.
pub fn format_pval(pvalues: &[f64], digits: usize) -> Vec<String>;

/// The cells R's printCoefmat(m, digits = 4, signif.stars = FALSE) prints for a table whose columns are an estimate,
/// its standard error, a test statistic, and a p-value. Cells are trimmed of padding.
pub struct CoefficientCells {
    pub estimates: Vec<String>,
    pub std_errors: Vec<String>,
    pub statistics: Vec<String>,
    pub p_values: Vec<String>,
}
pub fn coefficient_cells(estimates: &[f64], std_errors: &[f64], statistics: &[f64], p_values: &[f64])
    -> CoefficientCells;
```

## Decimal conversion

Every rounding below is the correctly rounded decimal value of the exact binary double. This is what C's `printf`
gives on glibc and macOS, and Rust's `format!("{:.N}")` and `format!("{:.Ne}")` give the same. An exact tie rounds to
even.

## format_real

The output is the format R chooses for the whole vector. Non-finite values do not take part in the choice.

For each finite x, round |x| to `digits` significant digits. Then:
- `e(x)` is the decimal exponent of the rounded value. For example, 99999.6 at 4 digits rounds to 1.000 × 10^5, so
  `e = 5`.
- `s(x)` is the number of significant digits left after dropping trailing zeros of the rounded mantissa, at least 1.
- Zero has `s = 1` and `e = 0`.

Fixed notation:
- Take the number of integer digits as `e(x) + 1`, less one when x printed with `max(0, digits - 1 - e(x))` decimals
  stays below `10^e(x)`. For example, at 4 digits, 99996 prints as `99996`, below 10^5, so it counts 5 integer
  digits. 99999.6 prints as `100000`, so it counts 6.
- Count at least one integer digit, plus one character for a minus sign.
- The decimals of the vector are the largest `s(x)` minus integer digits, and at least 0.
- The fixed width is the largest integer-part width, plus the decimals, plus 1 for the point when there are decimals.

Scientific notation:
- The mantissa has `m - 1` decimals, where m is the largest `s(x)`.
- The width is 1 for a minus sign if any value is negative, plus 1 digit, plus the point and `m - 1` decimals when
  `m > 1`, plus the exponent.
- The exponent is `e`, a sign, and two digits. When any value has `e >= 100` or `e <= -99`, count three digits.

Choose fixed notation when its width is at most the scientific width. This is R's default `scipen = 0`.

Printing:
- Fixed: each value is printed with the decimals of the vector.
- Scientific: each value is printed with `m - 1` mantissa decimals and an exponent of a sign and at least two digits,
  such as `1.5e-07`, `1e+05`, and `1e-300`.
- Negative zero prints as zero.
- NaN, positive infinity, and negative infinity print as `NaN`, `Inf`, and `-Inf`.
- All values are then padded on the left to the chosen notation's computed width, or to the widest string if
  that is wider. R pads to the computed width even when no value is that wide. For example, `c(-7e+07, 5e+100)` at 1
  digit has a computed scientific width of 7, so it prints as `" 5e+100"` and `" -7e+07"`.

`data/format.json` holds R's output for each input vector, given as strings of `%.17g` doubles. Every case must
match exactly.

## round

`round(x, d)` chooses between the two d-decimal neighbours of x:
1. Let `lo` and `hi` be the doubles nearest the d-decimal values just below and just above |x|. Parse each from its
   decimal string.
2. Return whichever is closer to |x|, comparing `|x| - lo` with `hi - |x|` in f64 arithmetic.
3. When the two differences are exactly equal, take the neighbour whose last decimal digit is even.
4. Give the result the sign of x. A rounded-away negative value gives `-0`.
5. When |x| is already a d-decimal value, it returns x.

This rule was inferred from R's results in `data/round.json`, and all of them must match exactly, sign of zero
included.

## format_g

C's `%.{digits}g`:
- Zero prints as `0`.
- NaN and the infinities print as `NaN`, `Inf`, and `-Inf`.
- Otherwise, with E the exponent of x rounded to `digits` significant digits, scientific style is used when
  `E < -4` or `E >= digits`, and fixed style otherwise.
- Trailing zeros, and then a trailing point, are removed from the mantissa or the fixed digits.
- The exponent has a sign and at least two digits.

`data/format_g.json` must match once R's output is trimmed. R's `formatC` also pads on the left to `digits + 1`
characters, and phit uses the unpadded form.

## format_pval

The machine epsilon here is 2.220446049250313e-16.

- NaN prints as `NA`.
- Values at least eps are split into two groups. Let `E = floor(log10(p))`. A value is in the fixed group when
  `E >= -3`, or when `E == -4` and `digits > 1`. The other values are in the second group. Each group is formatted
  with one `format_real(group, digits)` call.
- Values below eps print as `<`, a separator, and `format_real([eps], k)`, with `k = max(1, digits - 2)`:
  - If some values are at least eps, let w be the widest of their formatted strings. When `k > 1` and `k + 6 > w`,
    k becomes `max(1, w - 7)`. The separator is empty when `k == 1` and `w <= 6`, and a space otherwise.
  - If no value is at least eps, the separator is empty when `k == 1`, and a space otherwise.

`data/format_pval.json` must match exactly. The output strings there keep R's padding.

## coefficient_cells

This is R's `printCoefmat` at 4 digits, with no significance stars.

Estimates and standard errors are formatted together:
1. Find the smallest absolute value among all finite, nonzero estimates and standard errors. Let `g = 1 + floor(log10)`
   of it, or `g = 1` when there is none.
2. Round every estimate and standard error to `max(1, 4 - g)` decimals.
3. Format all 2p rounded values with one `format_real(·, 4)` call. The first p are the estimates and the rest the
   standard errors.

Statistics are rounded to 3 decimals, and then all of them are formatted with one `format_real(·, 4)` call.

P-values are `format_pval(p_values, 3)`.

Every cell is trimmed. A NaN estimate, standard error, or statistic prints `NA`, and a NaN p-value prints `NA` as
`format_pval` does.

`data/coefmat.json` holds random tables and R's printed rows: a header line, then one row per coefficient (`b1`, `b2`,
and so on), joined by single spaces with columns aligned. Check your cells against the rows. Splitting a row on runs
of spaces gives the row name and the four cells.
