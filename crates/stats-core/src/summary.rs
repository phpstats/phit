//! Text summaries of model results in the layout of R's `print(summary(fit))`, and the number formatting they use.
//!
//! The formatting follows R's `formatReal()` (via `format()`), `format.pval()`, and C's `%g` (via `formatC()`),
//! built on correctly rounded decimal conversion, so the output can be reproduced exactly elsewhere.

use crate::distributions::{FisherF, Normal, StudentT};

/// Significant digits for coefficient tables, as R's `max(3, getOption("digits") - 3)`.
pub const DIGITS: usize = 4;

/// Decimal digits of |x| rounded to `digits` significant digits: (mantissa digits, decimal exponent).
fn sig_digits(x: f64, digits: usize) -> (Vec<u8>, i32) {
    let s = format!("{:.*e}", digits - 1, x.abs());
    let (mantissa, exponent) = s.split_once('e').expect("exponent");
    (
        mantissa
            .bytes()
            .filter(u8::is_ascii_digit)
            .map(|b| b - b'0')
            .collect(),
        exponent.parse().expect("exponent"),
    )
}

/// R's `scientific()`: sign, decimal exponent, significant digits needed, and whether rounding adds a digit.
fn scientific(x: f64, digits: usize) -> (bool, i32, usize, bool) {
    if x == 0.0 {
        return (false, 0, 1, false);
    }
    let (mantissa, kpower) = sig_digits(x, digits);
    let mut nsig = digits;
    while nsig > 1 && mantissa[nsig - 1] == 0 {
        nsig -= 1;
    }
    // Fixed notation needs one digit fewer when |x|, shown with the decimals it would get there, stays below
    // 10^kpower (99996 at 4 digits prints as 99996, where 99999.6 rounds to 100000 and prints as 1e+05).
    let decimals = (digits as i32 - 1 - kpower).max(0) as usize;
    let shown: f64 = format!("{:.*}", decimals, x.abs())
        .parse()
        .expect("decimal");
    let widens = kpower > 0 && kpower <= 22 && shown < 10f64.powi(kpower);
    (x < 0.0, kpower, nsig, widens)
}

/// Exponent in R's style: a sign and at least two digits.
fn exponent(e: i32) -> String {
    format!("e{}{:02}", if e < 0 { '-' } else { '+' }, e.abs())
}

fn scientific_text(x: f64, decimals: usize) -> String {
    let s = format!("{:.*e}", decimals, x);
    let (mantissa, e) = s.split_once('e').expect("exponent");
    format!("{mantissa}{}", exponent(e.parse().expect("exponent")))
}

fn special(x: f64) -> Option<&'static str> {
    if x.is_nan() {
        Some("NaN")
    } else if x == f64::INFINITY {
        Some("Inf")
    } else if x == f64::NEG_INFINITY {
        Some("-Inf")
    } else {
        None
    }
}

/// R's `format(x, digits = digits)` for a vector: one common layout, fixed or scientific, padded to one width.
pub fn format_real(xs: &[f64], digits: usize) -> Vec<String> {
    let (mut neg, mut mxsl, mut rgt, mut mxns, mut mxe, mut mne) =
        (false, 1i32, 0i32, 0usize, i32::MIN, i32::MAX);
    let mut finite = false;
    for &x in xs.iter().filter(|x| x.is_finite()) {
        finite = true;
        let (negative, kpower, nsig, widens) = scientific(x, digits);
        let left = kpower + 1 - i32::from(widens);
        let sleft = i32::from(negative) + if left <= 0 { 1 } else { left };
        rgt = rgt.max(nsig as i32 - left);
        neg |= negative;
        mxsl = mxsl.max(sleft);
        mxns = mxns.max(nsig);
        mxe = mxe.max(kpower);
        mne = mne.min(kpower);
    }
    let rgt = rgt.max(0) as usize;
    let fixed_width = (mxsl as usize).max(1) + rgt + usize::from(rgt > 0);
    let mut scientific_layout = None;
    if finite {
        let e = if mxe >= 100 || mne <= -99 { 2 } else { 1 };
        let decimals = mxns - 1;
        let width = usize::from(neg) + usize::from(decimals > 0) + decimals + 4 + e;
        if fixed_width > width {
            scientific_layout = Some(decimals);
        }
    }
    let texts: Vec<String> = xs
        .iter()
        .map(|&x| match special(x) {
            Some(s) => s.to_string(),
            None => {
                let x = if x == 0.0 { 0.0 } else { x };
                match scientific_layout {
                    Some(decimals) => scientific_text(x, decimals),
                    None => format!("{:.*}", rgt, x),
                }
            }
        })
        .collect();
    let width = texts.iter().map(|s| s.chars().count()).max().unwrap_or(0);
    texts.into_iter().map(|s| format!("{s:>width$}")).collect()
}

/// R's `round(x, decimals)`, as the correctly rounded decimal.
pub fn round(x: f64, decimals: usize) -> f64 {
    if !x.is_finite() {
        return x;
    }
    format!("{:.*}", decimals, x).parse().expect("decimal")
}

/// C's `%.{digits}g`, as R's `formatC(x, digits = digits)`.
pub fn format_g(x: f64, digits: usize) -> String {
    if let Some(s) = special(x) {
        return s.to_string();
    }
    if x == 0.0 {
        return "0".into();
    }
    let (_, e) = sig_digits(x, digits);
    let trim = |s: String| {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_string()
        } else {
            s
        }
    };
    if e < -4 || e >= digits as i32 {
        let s = format!("{:.*e}", digits - 1, x);
        let (mantissa, _) = s.split_once('e').expect("exponent");
        format!("{}{}", trim(mantissa.to_string()), exponent(e))
    } else {
        trim(format!("{:.*}", (digits as i32 - 1 - e).max(0) as usize, x))
    }
}

/// R's `format.pval(pv, digits = digits)` with `eps` = machine epsilon.
pub fn format_pval(pvalues: &[f64], digits: usize) -> Vec<String> {
    let eps = f64::EPSILON;
    let mut out = vec![String::new(); pvalues.len()];
    let normal: Vec<usize> = (0..pvalues.len())
        .filter(|&i| !pvalues[i].is_nan() && pvalues[i] >= eps)
        .collect();
    let fixed: Vec<usize> = normal
        .iter()
        .copied()
        .filter(|&i| {
            let expo = pvalues[i].log10().floor();
            expo >= -3.0 || (expo == -4.0 && digits > 1)
        })
        .collect();
    let other: Vec<usize> = normal
        .iter()
        .copied()
        .filter(|i| !fixed.contains(i))
        .collect();
    for group in [&fixed, &other] {
        let texts = format_real(
            &group.iter().map(|&i| pvalues[i]).collect::<Vec<_>>(),
            digits,
        );
        for (&i, text) in group.iter().zip(texts) {
            out[i] = text;
        }
    }
    let tiny: Vec<usize> = (0..pvalues.len())
        .filter(|&i| !pvalues[i].is_nan() && pvalues[i] < eps)
        .collect();
    if !tiny.is_empty() {
        let mut eps_digits = digits.saturating_sub(2).max(1);
        let sep = if normal.is_empty() {
            if eps_digits == 1 { "" } else { " " }
        } else {
            let nc = normal
                .iter()
                .map(|&i| out[i].chars().count())
                .max()
                .unwrap_or(0);
            if eps_digits > 1 && eps_digits + 6 > nc {
                eps_digits = nc.saturating_sub(7).max(1);
            }
            if eps_digits == 1 && nc <= 6 { "" } else { " " }
        };
        let text = format!("<{sep}{}", format_real(&[eps], eps_digits)[0]);
        for i in tiny {
            out[i] = text.clone();
        }
    }
    for (i, p) in pvalues.iter().enumerate() {
        if p.is_nan() {
            out[i] = "NA".into();
        }
    }
    out
}

/// A text table: the first column left-aligned, the rest right-aligned unless listed in `left`, one space apart.
fn table(header: &[&str], rows: &[Vec<String>], left: &[usize]) -> String {
    let columns = header.len();
    let widths: Vec<usize> = (0..columns)
        .map(|c| {
            rows.iter()
                .map(|r| r[c].chars().count())
                .chain([header[c].chars().count()])
                .max()
                .unwrap_or(0)
        })
        .collect();
    let line = |cells: Vec<&str>| {
        let parts: Vec<String> = cells
            .iter()
            .enumerate()
            .map(|(c, s)| {
                let w = widths[c];
                if c == 0 || left.contains(&c) {
                    format!("{s:<w$}")
                } else {
                    format!("{s:>w$}")
                }
            })
            .collect();
        parts.join(" ").trim_end().to_string() + "\n"
    };
    let mut out = line(header.to_vec());
    for row in rows {
        out += &line(row.iter().map(String::as_str).collect());
    }
    out
}

/// Coefficient estimates with their standard errors, test statistics, and p-values.
pub struct Coefficients {
    pub names: Vec<String>,
    pub estimates: Vec<f64>,
    pub std_errors: Vec<f64>,
    pub statistics: Vec<f64>,
    pub p_values: Vec<f64>,
    /// 't' or 'z'.
    pub statistic: char,
}

/// R's `printCoefmat(digits = 4)` without significance stars.
pub fn coefficient_table(c: &Coefficients) -> String {
    let digits = DIGITS;
    let test_digits = digits - 1;
    let both: Vec<f64> = c.estimates.iter().chain(&c.std_errors).copied().collect();
    let sizes: Vec<f64> = both
        .iter()
        .map(|x| x.abs())
        .filter(|x| x.is_finite() && *x != 0.0)
        .collect();
    let digmin = sizes.iter().copied().fold(f64::INFINITY, f64::min);
    let digmin = if sizes.is_empty() {
        1
    } else {
        1 + digmin.log10().floor() as i32
    };
    let decimals = (digits as i32 - digmin).max(1) as usize;
    let estimate_text = format_real(
        &both.iter().map(|&x| round(x, decimals)).collect::<Vec<_>>(),
        digits,
    );
    let (estimates, errors) = estimate_text.split_at(c.estimates.len());
    let statistics = format_real(
        &c.statistics
            .iter()
            .map(|&x| round(x, test_digits))
            .collect::<Vec<_>>(),
        digits,
    );
    let p_values = format_pval(&c.p_values, test_digits);
    let na = |s: &String, x: f64| {
        if x.is_nan() {
            "NA".to_string()
        } else {
            s.trim().to_string()
        }
    };
    let rows: Vec<Vec<String>> = (0..c.names.len())
        .map(|i| {
            vec![
                c.names[i].clone(),
                na(&estimates[i], c.estimates[i]),
                na(&errors[i], c.std_errors[i]),
                na(&statistics[i], c.statistics[i]),
                p_values[i].trim().to_string(),
            ]
        })
        .collect();
    let (stat, p) = if c.statistic == 't' {
        ("t value", "Pr(>|t|)")
    } else {
        ("z value", "Pr(>|z|)")
    };
    table(&["", "Estimate", "Std. Error", stat, p], &rows, &[])
}

/// One random-effects term: its grouping factor, column names, covariance, and number of groups.
pub struct RandomTerm {
    pub group: String,
    pub columns: Vec<String>,
    pub covariance: Vec<Vec<f64>>,
    pub correlated: bool,
    pub n_groups: usize,
    /// Each group's conditional modes, when the fit reports them.
    pub values: Option<RandomValues>,
}

/// One random term's conditional modes and their conditional standard errors, as lme4's
/// `ranef(fit, condVar = TRUE)`: one row per group, one column per term column.
pub struct RandomValues {
    pub levels: Vec<String>,
    pub estimates: Vec<Vec<f64>>,
    pub std_errors: Vec<Vec<f64>>,
}

/// How one kind of fitted model prints, in the layout of R's `print(summary(fit))`. Each model kind implements it,
/// and [`render`] lays the parts out the same way for all of them, with the shared formatting above.
pub trait Printer {
    /// The heading: the fitting method, then the family and the formula when known. Each line ends in a newline.
    fn heading(&self) -> String;
    /// Fit statistics printed before the coefficient table, each line ending in a newline. Mixed models print
    /// their criteria and random effects here.
    fn before_coefficients(&self) -> String {
        String::new()
    }
    /// The coefficient table's title.
    fn coefficients_title(&self) -> &'static str {
        "Coefficients"
    }
    /// The coefficient estimates, standard errors, tests, and p-values.
    fn coefficients(&self) -> &Coefficients;
    /// Fit statistics printed after the coefficient table, each line ending in a newline.
    fn after_coefficients(&self) -> String {
        String::new()
    }
    /// Degrees of freedom for t confidence intervals, or `None` for normal intervals.
    fn interval_df(&self) -> Option<f64>;
    /// The random-effect terms of a mixed model, or `None` for a model without random effects.
    fn random_terms(&self) -> Option<&[RandomTerm]> {
        None
    }
}

/// The text of `print(summary(fit))` for any printer, ending with one newline.
pub fn render(p: &dyn Printer) -> String {
    let mut out = p.heading();
    out += &p.before_coefficients();
    out += &format!("\n{}:\n", p.coefficients_title());
    out += &coefficient_table(p.coefficients());
    out += &p.after_coefficients();
    out
}

fn heading(title: &str, family: Option<(&str, &str)>, formula: Option<&str>) -> String {
    let mut out = format!("{title}\n");
    if let Some((family, link)) = family {
        out += &format!("Family: {family} ({link})\n");
    }
    if let Some(formula) = formula {
        out += &format!("Formula: {formula}\n");
    }
    out
}

/// A linear model fitted by least squares, as R's `summary.lm`.
pub struct Linear {
    pub formula: Option<String>,
    pub coefficients: Coefficients,
    pub sigma: f64,
    pub df_residual: i64,
    pub r_squared: f64,
    pub adj_r_squared: f64,
    /// Numerator degrees of freedom of the overall F test, when the intercept is known.
    pub f_df: Option<i64>,
}

impl Printer for Linear {
    fn heading(&self) -> String {
        heading(
            "Linear model fit by least squares",
            None,
            self.formula.as_deref(),
        )
    }

    fn coefficients(&self) -> &Coefficients {
        &self.coefficients
    }

    fn after_coefficients(&self) -> String {
        let df = self.df_residual;
        let mut out = format!(
            "\nResidual standard error: {} on {df} degrees of freedom\n",
            format_real(&[self.sigma], DIGITS)[0]
        );
        out += &format!(
            "Multiple R-squared: {}, Adjusted R-squared: {}\n",
            format_g(self.r_squared, DIGITS),
            format_g(self.adj_r_squared, DIGITS)
        );
        if let Some(numerator) = self.f_df.filter(|d| *d > 0) {
            let r2 = self.r_squared;
            let f = (r2 / numerator as f64) / ((1.0 - r2) / df as f64);
            let p = FisherF::new(numerator as f64, df as f64)
                .map(|d| d.sf(f))
                .unwrap_or(f64::NAN);
            out += &format!(
                "F-statistic: {} on {numerator} and {df} DF, p-value: {}\n",
                format_g(f, DIGITS),
                format_pval(&[p], DIGITS)[0]
            );
        }
        out
    }

    fn interval_df(&self) -> Option<f64> {
        Some(self.df_residual as f64)
    }
}

/// A generalized linear model fitted by maximum likelihood, as R's `summary.glm`, for any family and link.
pub struct GeneralizedLinear {
    /// The family and link, when the fit reports them.
    pub family: Option<(String, String)>,
    pub formula: Option<String>,
    pub coefficients: Coefficients,
    pub dispersion: f64,
    pub null_deviance: f64,
    pub deviance: f64,
    pub df_null: Option<i64>,
    pub df_residual: i64,
    pub aic: f64,
}

impl Printer for GeneralizedLinear {
    fn heading(&self) -> String {
        heading(
            "Generalized linear model fit by maximum likelihood",
            self.family.as_ref().map(|(f, l)| (f.as_str(), l.as_str())),
            self.formula.as_deref(),
        )
    }

    fn coefficients(&self) -> &Coefficients {
        &self.coefficients
    }

    fn after_coefficients(&self) -> String {
        let family = self
            .family
            .as_ref()
            .map(|(f, _)| format!(" for {f} family"))
            .unwrap_or_default();
        let mut out = format!(
            "\n(Dispersion parameter{family} taken to be {})\n\n",
            format_real(&[self.dispersion], 7)[0]
        );
        let deviances = format_real(&[self.null_deviance, self.deviance], DIGITS + 1);
        match self.df_null {
            Some(df_null) => {
                let dfs = format_real(&[df_null as f64, self.df_residual as f64], 15);
                out += &format!(
                    "    Null deviance: {} on {} degrees of freedom\n",
                    deviances[0], dfs[0]
                );
                out += &format!(
                    "Residual deviance: {} on {} degrees of freedom\n",
                    deviances[1], dfs[1]
                );
            }
            None => {
                out += &format!("    Null deviance: {}\n", deviances[0]);
                out += &format!(
                    "Residual deviance: {} on {} degrees of freedom\n",
                    deviances[1], self.df_residual
                );
            }
        }
        out + &format!("AIC: {}\n", format_real(&[self.aic], DIGITS + 1)[0])
    }

    fn interval_df(&self) -> Option<f64> {
        (self.coefficients.statistic == 't').then_some(self.df_residual as f64)
    }
}

/// What every mixed model prints before its fixed effects.
pub struct MixedFit {
    pub log_likelihood: f64,
    pub aic: Option<f64>,
    pub bic: Option<f64>,
    pub df_residual: i64,
    pub n_observations: i64,
    pub terms: Vec<RandomTerm>,
    pub singular: bool,
}

impl MixedFit {
    /// The ML criteria table, as lme4 prints it: one decimal each.
    fn criteria(&self) -> String {
        let mut names = vec![];
        let mut values = vec![];
        if let Some(a) = self.aic {
            names.push("AIC");
            values.push(a);
        }
        if let Some(b) = self.bic {
            names.push("BIC");
            values.push(b);
        }
        names.extend(["logLik", "deviance"]);
        values.extend([self.log_likelihood, -2.0 * self.log_likelihood]);
        let mut texts: Vec<String> = values.iter().map(|v| format!("{v:.1}")).collect();
        names.push("df.resid");
        texts.push(self.df_residual.to_string());
        let widths: Vec<usize> = names
            .iter()
            .zip(&texts)
            .map(|(n, t)| n.len().max(t.len()))
            .collect();
        let header: Vec<String> = names
            .iter()
            .zip(&widths)
            .map(|(n, w)| format!("{n:>w$}"))
            .collect();
        let row: Vec<String> = texts
            .iter()
            .zip(&widths)
            .map(|(t, w)| format!("{t:>w$}"))
            .collect();
        format!("\n{}\n{}\n", header.join(" "), row.join(" "))
    }

    /// The random-effects table and the observation and group counts.
    fn random(&self, residual: Option<f64>) -> String {
        let groups: Vec<String> = self
            .terms
            .iter()
            .fold(Vec::<(String, usize)>::new(), |mut seen, t| {
                if !seen.iter().any(|(g, _)| g == &t.group) {
                    seen.push((t.group.clone(), t.n_groups));
                }
                seen
            })
            .into_iter()
            .map(|(g, n)| format!("{g}, {n}"))
            .collect();
        format!(
            "\nRandom effects:\n{}Number of obs: {}, groups: {}\n",
            random_effects(&self.terms, residual),
            self.n_observations,
            groups.join("; ")
        )
    }

    fn singular_note(&self) -> String {
        if self.singular {
            "\nThe fit is singular: a random-effect variance or correlation is at its boundary.\n"
                .into()
        } else {
            String::new()
        }
    }
}

/// A Gaussian linear mixed model, as lme4's `summary.merMod` for `lmer()`.
pub struct MixedGaussian {
    pub formula: Option<String>,
    pub reml: bool,
    pub fit: MixedFit,
    pub residual_variance: f64,
    pub coefficients: Coefficients,
}

impl Printer for MixedGaussian {
    fn heading(&self) -> String {
        let method = if self.reml {
            "REML"
        } else {
            "maximum likelihood"
        };
        heading(
            &format!("Linear mixed model fit by {method}"),
            None,
            self.formula.as_deref(),
        )
    }

    fn before_coefficients(&self) -> String {
        let criteria = if self.reml {
            format!(
                "\nREML criterion at convergence: {}\n",
                format_real(&[-2.0 * self.fit.log_likelihood], DIGITS + 1)[0]
            )
        } else {
            self.fit.criteria()
        };
        criteria + &self.fit.random(Some(self.residual_variance))
    }

    fn coefficients_title(&self) -> &'static str {
        "Fixed effects"
    }

    fn coefficients(&self) -> &Coefficients {
        &self.coefficients
    }

    fn after_coefficients(&self) -> String {
        self.fit.singular_note()
    }

    fn interval_df(&self) -> Option<f64> {
        None
    }

    fn random_terms(&self) -> Option<&[RandomTerm]> {
        Some(&self.fit.terms)
    }
}

/// A binomial logit mixed model, as lme4's `summary.merMod` for `glmer()`.
pub struct MixedLogistic {
    pub formula: Option<String>,
    /// The integration method, such as "Laplace approximation".
    pub integration: String,
    pub fit: MixedFit,
    pub coefficients: Coefficients,
}

impl Printer for MixedLogistic {
    fn heading(&self) -> String {
        heading(
            &format!(
                "Generalized linear mixed model fit by maximum likelihood ({})",
                self.integration
            ),
            Some(("binomial", "logit")),
            self.formula.as_deref(),
        )
    }

    fn before_coefficients(&self) -> String {
        self.fit.criteria() + &self.fit.random(None)
    }

    fn coefficients_title(&self) -> &'static str {
        "Fixed effects"
    }

    fn coefficients(&self) -> &Coefficients {
        &self.coefficients
    }

    fn after_coefficients(&self) -> String {
        self.fit.singular_note()
    }

    fn interval_df(&self) -> Option<f64> {
        None
    }

    fn random_terms(&self) -> Option<&[RandomTerm]> {
        Some(&self.fit.terms)
    }
}

fn random_effects(terms: &[RandomTerm], residual: Option<f64>) -> String {
    let mut rows = vec![];
    let mut variances = vec![];
    let mut correlations: Vec<Vec<String>> = vec![];
    let width = terms
        .iter()
        .map(|t| if t.correlated { t.columns.len() } else { 1 })
        .max()
        .unwrap_or(1)
        - 1;
    for term in terms {
        for (j, name) in term.columns.iter().enumerate() {
            rows.push(vec![
                if j == 0 {
                    term.group.clone()
                } else {
                    String::new()
                },
                name.clone(),
            ]);
            variances.push(term.covariance[j][j]);
            let mut corr = vec![String::new(); width];
            if term.correlated {
                for (m, cell) in corr.iter_mut().enumerate().take(j) {
                    let r = term.covariance[j][m]
                        / (term.covariance[j][j] * term.covariance[m][m]).sqrt();
                    *cell = format!("{:.2}", if r == 0.0 { 0.0 } else { r });
                }
            }
            correlations.push(corr);
        }
    }
    if let Some(v) = residual {
        rows.push(vec!["Residual".into(), String::new()]);
        variances.push(v);
        correlations.push(vec![String::new(); width]);
    }
    let sds: Vec<f64> = variances.iter().map(|v| v.sqrt()).collect();
    let (vt, st) = (format_real(&variances, DIGITS), format_real(&sds, DIGITS));
    for (i, row) in rows.iter_mut().enumerate() {
        row.push(vt[i].trim().to_string());
        row.push(st[i].trim().to_string());
        row.extend(correlations[i].iter().cloned());
    }
    let mut header = vec!["Groups", "Name", "Variance", "Std.Dev."];
    header.extend(std::iter::repeat_n("", width));
    if width > 0 {
        header[4] = "Corr";
    }
    table(&header, &rows, &[1])
}

/// One column of a printed table: text, or numbers formatted together. `None` is a missing value.
pub enum Cells {
    Text(Vec<Option<String>>),
    Numbers(Vec<Option<f64>>),
}

/// A table in the layout of R's `print.data.frame()`: row numbers left-aligned in an unnamed first column, every
/// other column right-aligned under its name, one space apart. Numbers are formatted per column as
/// `format(x, digits = 7)`, a missing number prints `NA` and a missing string `<NA>`. As R does at its default width
/// of 80, columns are split into blocks whose lines hold at most 79 characters, each block with its own header and
/// row numbers; a column wider than that is a block of its own.
pub fn data_frame(names: &[String], columns: &[Cells]) -> String {
    let rows = columns.first().map_or(0, |c| match c {
        Cells::Text(v) => v.len(),
        Cells::Numbers(v) => v.len(),
    });
    if rows == 0 {
        return format!("<0 rows> {}\n", names.join(", "));
    }
    let texts: Vec<Vec<String>> = columns
        .iter()
        .map(|column| match column {
            Cells::Text(values) => values
                .iter()
                .map(|v| v.clone().unwrap_or_else(|| "<NA>".into()))
                .collect(),
            Cells::Numbers(values) => {
                let present: Vec<f64> = values.iter().flatten().copied().collect();
                let mut formatted = format_real(&present, 7).into_iter();
                values
                    .iter()
                    .map(|v| match v {
                        Some(_) => formatted.next().unwrap_or_default(),
                        None => "NA".into(),
                    })
                    .collect()
            }
        })
        .collect();
    let labels: Vec<String> = (1..=rows).map(|i| i.to_string()).collect();
    let label_width = labels.iter().map(String::len).max().unwrap_or(0);
    let widths: Vec<usize> = names
        .iter()
        .zip(&texts)
        .map(|(name, cells)| {
            cells
                .iter()
                .map(|c| c.chars().count())
                .chain([name.chars().count()])
                .max()
                .unwrap_or(0)
        })
        .collect();
    // Blocks of columns, filled greedily while a line stays within 79 characters.
    let mut blocks: Vec<std::ops::Range<usize>> = vec![];
    let mut start = 0;
    while start < names.len() {
        let mut end = start + 1;
        let mut line = label_width + 1 + widths[start];
        while end < names.len() && line + 1 + widths[end] <= 79 {
            line += 1 + widths[end];
            end += 1;
        }
        blocks.push(start..end);
        start = end;
    }
    let mut out = String::new();
    for block in blocks {
        out += &" ".repeat(label_width);
        for c in block.clone() {
            out += &format!(" {:>w$}", names[c], w = widths[c]);
        }
        out += "\n";
        for (r, label) in labels.iter().enumerate() {
            out += &format!("{label:<label_width$}");
            for c in block.clone() {
                out += &format!(" {:>w$}", texts[c][r].trim(), w = widths[c]);
            }
            out += "\n";
        }
    }
    out
}

/// Wald confidence intervals: t with `df` degrees of freedom when given, else the standard normal.
pub fn confidence_intervals(
    estimates: &[f64],
    std_errors: &[f64],
    df: Option<f64>,
    level: f64,
) -> Vec<(f64, f64)> {
    let p = 0.5 + level / 2.0;
    let q = match df {
        Some(df) => StudentT::new(df).map(|t| t.quantile(p)).unwrap_or(f64::NAN),
        None => Normal::new(0.0, 1.0)
            .map(|n| n.quantile(p))
            .unwrap_or(f64::NAN),
    };
    estimates
        .iter()
        .zip(std_errors)
        .map(|(b, se)| (b - q * se, b + q * se))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn real_formatting_follows_r() {
        assert_eq!(
            format_real(&[33.9908, -3.2056, 1.8878, 0.7539], 4),
            ["33.9908", "-3.2056", " 1.8878", " 0.7539"]
        );
        assert_eq!(format_real(&[1378.2, 960.5], 4), ["1378.2", " 960.5"]);
        assert_eq!(format_real(&[2.220446049250313e-16], 1), ["2e-16"]);
        assert_eq!(format_real(&[0.0, -0.0], 4), ["0", "0"]);
        assert_eq!(
            format_real(&[1.0, f64::NAN, f64::INFINITY], 4),
            ["  1", "NaN", "Inf"]
        );
        assert_eq!(format_real(&[123456789.0], 4), ["123456789"]);
        assert_eq!(format_real(&[0.000123456], 4), ["0.0001235"]);
        assert_eq!(format_real(&[1e-10, 0.5], 4), ["1e-10", "5e-01"]);
        assert_eq!(
            format_real(&[-0.00001234, 12.0], 4),
            ["-1.234e-05", " 1.200e+01"]
        );
        assert_eq!(format_real(&[0.1 + 0.2], 4), ["0.3"]);
        assert_eq!(format_real(&[100.0, 1e-8], 4), ["1e+02", "1e-08"]);
        assert_eq!(format_real(&[9999.6], 4), ["10000"]);
        assert_eq!(format_real(&[999.96], 4), ["1000"]);
        assert_eq!(format_real(&[99999.6], 4), ["1e+05"]);
        assert_eq!(format_real(&[99999.6, 1.5], 4), ["1.0e+05", "1.5e+00"]);
        assert_eq!(format_real(&[99996.0], 4), ["99996"]);
        assert_eq!(format_real(&[9999.6, 1.0], 4), ["10000", "    1"]);
    }

    #[test]
    fn p_values_follow_format_pval() {
        assert_eq!(
            format_pval(&[1e-20, 0.000213, 0.004718, 0.000999], 3),
            ["< 2e-16", "0.000213", "0.004718", "0.000999"]
        );
        assert_eq!(format_pval(&[3.594e-11], 4), ["3.594e-11"]);
        assert_eq!(format_pval(&[1e-20], 3), ["<2e-16"]);
        assert_eq!(
            format_pval(&[0.5, 1e-5, 1e-20], 3),
            ["0.5", "1e-05", "<2e-16"]
        );
        assert_eq!(format_pval(&[0.04, 0.3], 3), ["0.04", "0.30"]);
        assert_eq!(format_pval(&[f64::NAN], 3), ["NA"]);
    }

    #[test]
    fn data_frame_follows_print_data_frame() {
        let table = data_frame(
            &["term".into(), "estimate".into()],
            &[
                Cells::Text(vec![Some("(Intercept)".into()), Some("wt".into())]),
                Cells::Numbers(vec![Some(37.28512616734203), Some(-5.344471572722675)]),
            ],
        );
        assert_eq!(
            table,
            "         term  estimate\n1 (Intercept) 37.285126\n2          wt -5.344472\n"
        );
    }

    #[test]
    fn g_follows_c() {
        assert_eq!(format_g(0.83743, 4), "0.8374");
        assert_eq!(format_g(0.82, 4), "0.82");
        assert_eq!(format_g(48.08, 4), "48.08");
        assert_eq!(format_g(1.5e-7, 4), "1.5e-07");
        assert_eq!(format_g(123456.0, 4), "1.235e+05");
        assert_eq!(format_g(0.0001234, 4), "0.0001234");
        assert_eq!(format_g(100.0, 4), "100");
    }
}
