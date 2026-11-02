//! Written from the clean-room specification specs/cleanroom/format.md, by an implementer who had not seen R's source
//! or phit's earlier code; sources: R's documentation and R's observed output (specs/cleanroom/data). See THIRD_PARTY_NOTICES.md.

pub const DIGITS: usize = 4;

/// Machine epsilon used by `format_pval`.
const PVAL_EPS: f64 = f64::EPSILON;

/// Text for a non-finite value.
fn non_finite_text(x: f64) -> &'static str {
    if x.is_nan() {
        "NaN"
    } else if x > 0.0 {
        "Inf"
    } else {
        "-Inf"
    }
}

/// Split Rust's `{:e}` output ("1.234e-5") into mantissa text and exponent.
fn split_sci(text: &str) -> (&str, i32) {
    let pos = text.find('e').expect("scientific text has an exponent");
    let exp: i32 = text[pos + 1..].parse().expect("exponent parses");
    (&text[..pos], exp)
}

/// Exponent text in C style: a sign and at least two digits.
fn exponent_text(exp: i32) -> String {
    let sign = if exp < 0 { '-' } else { '+' };
    format!("{}{:02}", sign, exp.unsigned_abs())
}

/// Round |x| to `sig` significant digits. Returns (significant digits left after dropping
/// trailing zeros, decimal exponent of the rounded value).
fn sig_and_exp(x: f64, sig: usize) -> (usize, i32) {
    if x == 0.0 {
        return (1, 0);
    }
    let text = format!("{:.*e}", sig.max(1) - 1, x.abs());
    let (mantissa, exp) = split_sci(&text);
    let digits: Vec<u8> = mantissa.bytes().filter(|b| b.is_ascii_digit()).collect();
    let kept = digits.iter().rposition(|&d| d != b'0').map_or(1, |i| i + 1);
    (kept.max(1), exp)
}

/// Print `x` in scientific notation with `decimals` mantissa decimals and a C-style exponent.
fn sci_text(x: f64, decimals: usize) -> String {
    let x = if x == 0.0 { 0.0 } else { x };
    let text = format!("{:.*e}", decimals, x);
    let (mantissa, exp) = split_sci(&text);
    format!("{}e{}", mantissa, exponent_text(exp))
}

/// Print `x` in fixed notation with `decimals` decimals; negative zero prints as zero.
fn fixed_text(x: f64, decimals: usize) -> String {
    let x = if x == 0.0 { 0.0 } else { x };
    format!("{:.*}", decimals, x)
}

fn pad_left(cells: Vec<String>, min_width: usize) -> Vec<String> {
    let width = cells
        .iter()
        .map(|c| c.chars().count())
        .max()
        .unwrap_or(0)
        .max(min_width);
    cells
        .into_iter()
        .map(|c| format!("{:>width$}", c, width = width))
        .collect()
}

/// R's format(xs, digits = digits): every element in one common notation and padded on the left to one width.
pub fn format_real(xs: &[f64], digits: usize) -> Vec<String> {
    let digits = digits.max(1);

    let mut any_finite = false;
    let mut any_negative = false;
    let mut max_sig = 1usize;
    let mut int_width = 1usize;
    let mut max_decimals: i64 = 0;
    let mut wide_exponent = false;

    for &x in xs.iter().filter(|x| x.is_finite()) {
        any_finite = true;
        let negative = x < 0.0;
        any_negative |= negative;
        let (sig, exp) = sig_and_exp(x, digits);
        max_sig = max_sig.max(sig);
        if exp >= 100 || exp <= -99 {
            wide_exponent = true;
        }

        // Integer digits in fixed notation.
        let mut int_digits = exp as i64 + 1;
        let probe_decimals = (digits as i64 - 1 - exp as i64).max(0) as usize;
        let printed: f64 = fixed_text(x.abs(), probe_decimals)
            .parse()
            .unwrap_or(x.abs());
        if printed < 10f64.powi(exp) {
            int_digits -= 1;
        }
        let width = int_digits.max(1) as usize + usize::from(negative);
        int_width = int_width.max(width);

        let decimals = sig as i64 - 1 - exp as i64;
        max_decimals = max_decimals.max(decimals);
    }

    let mut notation_width = 0;
    let cells: Vec<String> = if !any_finite {
        xs.iter().map(|&x| non_finite_text(x).to_string()).collect()
    } else {
        let decimals = max_decimals.max(0) as usize;
        let fixed_width = int_width + decimals + usize::from(decimals > 0);
        let mantissa_decimals = max_sig - 1;
        let sci_width = usize::from(any_negative)
            + 1
            + if max_sig > 1 {
                1 + mantissa_decimals
            } else {
                0
            }
            + if wide_exponent { 5 } else { 4 };
        let use_fixed = fixed_width <= sci_width;
        notation_width = if use_fixed { fixed_width } else { sci_width };
        xs.iter()
            .map(|&x| {
                if !x.is_finite() {
                    non_finite_text(x).to_string()
                } else if use_fixed {
                    fixed_text(x, decimals)
                } else {
                    sci_text(x, mantissa_decimals)
                }
            })
            .collect()
    };
    pad_left(cells, notation_width)
}

/// Add one to (`up`) or subtract one from a string of decimal digits.
fn step_digits(digits: &str, up: bool) -> String {
    let mut bytes: Vec<u8> = digits.bytes().map(|b| b - b'0').collect();
    let (edge, reset) = if up { (9, 0) } else { (0, 9) };
    let mut i = bytes.len();
    loop {
        if i == 0 {
            // Only reachable going up past all nines.
            bytes.insert(0, 1);
            break;
        }
        i -= 1;
        if bytes[i] == edge {
            bytes[i] = reset;
        } else if up {
            bytes[i] += 1;
            break;
        } else {
            bytes[i] -= 1;
            break;
        }
    }
    bytes.iter().map(|d| char::from(b'0' + d)).collect()
}

/// R's round(x, decimals).
pub fn round(x: f64, decimals: usize) -> f64 {
    if !x.is_finite() {
        return x;
    }
    let magnitude = x.abs();
    let nearest_text = format!("{:.*}", decimals, magnitude);
    let nearest: f64 = nearest_text.parse().expect("decimal text parses");
    if nearest == magnitude {
        // |x| is already a d-decimal value.
        return x;
    }
    // Scaled digits of the nearest d-decimal value, then its neighbour on the other side of |x|.
    let scaled: String = nearest_text.chars().filter(|c| *c != '.').collect();
    let (lo_digits, hi_digits) = if nearest < magnitude {
        let hi = step_digits(&scaled, true);
        (scaled, hi)
    } else {
        let lo = step_digits(&scaled, false);
        (lo, scaled)
    };
    let to_double = |digits: &str| -> f64 {
        format!("{}e-{}", digits, decimals)
            .parse()
            .expect("neighbour parses")
    };
    let (lo, hi) = (to_double(&lo_digits), to_double(&hi_digits));
    let (below, above) = (magnitude - lo, hi - magnitude);
    let lo_even = lo_digits
        .bytes()
        .last()
        .is_some_and(|b| (b - b'0').is_multiple_of(2));
    let chosen = if below < above || (below == above && lo_even) {
        lo
    } else {
        hi
    };
    chosen.copysign(x)
}

/// Remove trailing zeros, and then a trailing point, from a decimal digit string.
fn strip_trailing_zeros(text: &str) -> &str {
    if !text.contains('.') {
        return text;
    }
    text.trim_end_matches('0').trim_end_matches('.')
}

/// R's formatC(x, digits = digits), which is C's printf("%.{digits}g").
pub fn format_g(x: f64, digits: usize) -> String {
    if !x.is_finite() {
        return non_finite_text(x).to_string();
    }
    if x == 0.0 {
        return "0".to_string();
    }
    let precision = digits.max(1);
    let rounded = format!("{:.*e}", precision - 1, x);
    let (mantissa, exp) = split_sci(&rounded);
    if exp < -4 || exp >= precision as i32 {
        format!("{}e{}", strip_trailing_zeros(mantissa), exponent_text(exp))
    } else {
        let decimals = (precision as i32 - 1 - exp) as usize;
        strip_trailing_zeros(&format!("{:.*}", decimals, x)).to_string()
    }
}

/// R's format.pval(pvalues, digits = digits) with eps = f64::EPSILON.
pub fn format_pval(pvalues: &[f64], digits: usize) -> Vec<String> {
    let mut out: Vec<String> = vec![String::new(); pvalues.len()];

    let mut fixed_idx = Vec::new();
    let mut sci_idx = Vec::new();
    let mut tiny_idx = Vec::new();
    for (i, &p) in pvalues.iter().enumerate() {
        if p.is_nan() {
            out[i] = "NA".to_string();
        } else if p >= PVAL_EPS {
            let e = p.log10().floor() as i64;
            if e >= -3 || (e == -4 && digits > 1) {
                fixed_idx.push(i);
            } else {
                sci_idx.push(i);
            }
        } else {
            tiny_idx.push(i);
        }
    }

    let mut widest = 0usize;
    for group in [&fixed_idx, &sci_idx] {
        if group.is_empty() {
            continue;
        }
        let values: Vec<f64> = group.iter().map(|&i| pvalues[i]).collect();
        for (&i, cell) in group.iter().zip(format_real(&values, digits)) {
            widest = widest.max(cell.chars().count());
            out[i] = cell;
        }
    }

    if !tiny_idx.is_empty() {
        let mut k = digits.saturating_sub(2).max(1);
        let separator = if fixed_idx.is_empty() && sci_idx.is_empty() {
            if k == 1 { "" } else { " " }
        } else {
            if k > 1 && k + 6 > widest {
                k = widest.saturating_sub(7).max(1);
            }
            if k == 1 && widest <= 6 { "" } else { " " }
        };
        let eps_text = format_real(&[PVAL_EPS], k).remove(0);
        let cell = format!("<{}{}", separator, eps_text);
        for i in tiny_idx {
            out[i] = cell.clone();
        }
    }
    out
}

/// The cells R's printCoefmat(m, digits = 4, signif.stars = FALSE) prints for a table whose columns are an estimate,
/// its standard error, a test statistic, and a p-value. Cells are trimmed of padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoefficientCells {
    pub estimates: Vec<String>,
    pub std_errors: Vec<String>,
    pub statistics: Vec<String>,
    pub p_values: Vec<String>,
}

/// Format a column with `format_real` at `DIGITS`, trimming cells and printing NaN as NA.
fn column_cells(values: &[f64]) -> Vec<String> {
    format_real(values, DIGITS)
        .into_iter()
        .zip(values)
        .map(|(cell, &v)| {
            if v.is_nan() {
                "NA".to_string()
            } else {
                cell.trim().to_string()
            }
        })
        .collect()
}

pub fn coefficient_cells(
    estimates: &[f64],
    std_errors: &[f64],
    statistics: &[f64],
    p_values: &[f64],
) -> CoefficientCells {
    let smallest = estimates
        .iter()
        .chain(std_errors)
        .filter(|v| v.is_finite() && **v != 0.0)
        .map(|v| v.abs())
        .fold(f64::INFINITY, f64::min);
    let g: i64 = if smallest.is_finite() {
        1 + smallest.log10().floor() as i64
    } else {
        1
    };
    let decimals = (DIGITS as i64 - g).max(1) as usize;

    let joint: Vec<f64> = estimates
        .iter()
        .chain(std_errors)
        .map(|&v| round(v, decimals))
        .collect();
    let mut estimate_cells = column_cells(&joint);
    let std_error_cells = estimate_cells.split_off(estimates.len());

    let rounded_stats: Vec<f64> = statistics.iter().map(|&v| round(v, 3)).collect();

    CoefficientCells {
        estimates: estimate_cells,
        std_errors: std_error_cells,
        statistics: column_cells(&rounded_stats),
        p_values: format_pval(p_values, 3)
            .into_iter()
            .map(|c| c.trim().to_string())
            .collect(),
    }
}
