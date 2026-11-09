//! PHP bindings for model summaries: Phit\summary() prints a fit in the layout of R's print(summary(fit)), and
//! Phit\tidy() returns its coefficient table as columns, as broom's tidy() does. Both pick the fit's printer from
//! its `model` key through PRINTERS; the layouts are the stats_core::summary::Printer implementations.
use crate::stats_exception;
use ext_php_rs::{boxed::ZBox, prelude::*, types::ZendHashTable, types::Zval};
use stats_core::summary::{
    self, Coefficients, GeneralizedLinear, Linear, MixedFit, MixedGaussian, MixedLogistic, Printer,
    RandomTerm, RandomValues,
};
use stats_core::{ErrorKind, StatsError};

fn invalid(message: &str) -> PhpException {
    stats_exception(StatsError::new(ErrorKind::InvalidInput, message))
}

fn not_a_fit(detail: &str) -> PhpException {
    invalid(&format!(
        "fit must be a result of a Phit model function: {detail}"
    ))
}

fn float(fit: &ZendHashTable, key: &str) -> Option<f64> {
    let v = fit.get(key)?;
    v.double().or_else(|| v.long().map(|l| l as f64))
}

fn required_float(fit: &ZendHashTable, key: &str) -> PhpResult<f64> {
    float(fit, key).ok_or_else(|| not_a_fit(&format!("missing {key}")))
}

fn long(fit: &ZendHashTable, key: &str) -> Option<i64> {
    let v = fit.get(key)?;
    v.long()
        .or_else(|| v.double().filter(|d| d.fract() == 0.0).map(|d| d as i64))
}

fn text(fit: &ZendHashTable, key: &str) -> Option<String> {
    fit.get(key).and_then(Zval::str).map(str::to_owned)
}

fn floats(value: Option<&Zval>, key: &str) -> PhpResult<Vec<f64>> {
    value
        .and_then(Zval::array)
        .ok_or_else(|| not_a_fit(&format!("missing {key}")))?
        .values()
        .map(|v| {
            if v.is_null() {
                Ok(f64::NAN)
            } else {
                v.double()
                    .or_else(|| v.long().map(|l| l as f64))
                    .ok_or_else(|| not_a_fit(&format!("{key} must be numbers")))
            }
        })
        .collect()
}

fn strings(value: Option<&Zval>) -> Option<Vec<String>> {
    value?
        .array()?
        .values()
        .map(|v| v.str().map(str::to_owned))
        .collect()
}

fn matrix(value: Option<&Zval>, key: &str) -> PhpResult<Vec<Vec<f64>>> {
    value
        .and_then(Zval::array)
        .ok_or_else(|| not_a_fit(&format!("missing {key}")))?
        .values()
        .map(|row| floats(Some(row), key))
        .collect()
}

/// The coefficient table of any fit: names from `columns`, or x1, x2, ... for a matrix fit.
fn coefficients(fit: &ZendHashTable) -> PhpResult<Coefficients> {
    let estimates = floats(fit.get("coefficients"), "coefficients")?;
    let (statistics, statistic) = match fit.get("t_statistics") {
        Some(v) => (floats(Some(v), "t_statistics")?, 't'),
        None => (floats(fit.get("z_statistics"), "z_statistics")?, 'z'),
    };
    let names = strings(fit.get("columns"))
        .unwrap_or_else(|| (1..=estimates.len()).map(|i| format!("x{i}")).collect());
    let std_errors = floats(fit.get("std_errors"), "std_errors")?;
    let p_values = floats(fit.get("p_values"), "p_values")?;
    if [
        names.len(),
        std_errors.len(),
        statistics.len(),
        p_values.len(),
    ]
    .iter()
    .any(|&n| n != estimates.len())
    {
        return Err(not_a_fit("coefficient lists differ in length"));
    }
    Ok(Coefficients {
        names,
        estimates,
        std_errors,
        statistics,
        p_values,
        statistic,
    })
}

/// Group labels as text: lme4 prints integer and string labels alike.
fn labels(value: Option<&Zval>) -> Option<Vec<String>> {
    value?
        .array()?
        .values()
        .map(|v| {
            v.str()
                .map(str::to_owned)
                .or_else(|| v.long().map(|l| l.to_string()))
                .or_else(|| v.double().map(|d| d.to_string()))
        })
        .collect()
}

/// A term's conditional modes and standard errors, when the fit has them: `effects` rows with `effects_cond_var`
/// k×k matrices, or (`flat`) one value per group with one variance per group.
fn random_values(
    labels: Option<Vec<String>>,
    effects: Option<&Zval>,
    cond_var: Option<&Zval>,
    k: usize,
    flat: bool,
) -> PhpResult<Option<RandomValues>> {
    let (Some(levels), Some(_), Some(_)) = (labels, effects, cond_var) else {
        return Ok(None);
    };
    let (estimates, std_errors): (Vec<Vec<f64>>, Vec<Vec<f64>>) = if flat {
        let e = floats(effects, "random_effects")?;
        let v = floats(cond_var, "random_effects_cond_var")?;
        if v.len() != e.len() {
            return Err(not_a_fit(
                "random effects and their variances differ in length",
            ));
        }
        (
            e.into_iter().map(|x| vec![x]).collect(),
            v.into_iter().map(|x| vec![x.sqrt()]).collect(),
        )
    } else {
        let e = matrix(effects, "effects")?;
        let v = cond_var
            .and_then(Zval::array)
            .ok_or_else(|| not_a_fit("missing effects_cond_var"))?
            .values()
            .map(|m| matrix(Some(m), "effects_cond_var"))
            .collect::<PhpResult<Vec<_>>>()?;
        if v.len() != e.len()
            || v.iter()
                .any(|m| m.len() != k || m.iter().any(|r| r.len() != k))
        {
            return Err(not_a_fit(
                "random effects and their variances differ in size",
            ));
        }
        let se = v
            .iter()
            .map(|m| (0..k).map(|j| m[j][j].sqrt()).collect())
            .collect();
        (e, se)
    };
    if estimates.len() != levels.len() || estimates.iter().any(|r| r.len() != k) {
        return Err(not_a_fit("random effects and group labels differ in size"));
    }
    Ok(Some(RandomValues {
        levels,
        estimates,
        std_errors,
    }))
}

fn random_terms(fit: &ZendHashTable) -> PhpResult<Vec<RandomTerm>> {
    let one_intercept = |group: String| -> PhpResult<Vec<RandomTerm>> {
        Ok(vec![RandomTerm {
            group,
            columns: vec!["(Intercept)".into()],
            covariance: vec![vec![required_float(fit, "random_intercept_variance")?]],
            correlated: false,
            n_groups: long(fit, "n_groups").ok_or_else(|| not_a_fit("missing n_groups"))? as usize,
            values: random_values(
                labels(fit.get("group_labels")),
                fit.get("random_effects"),
                fit.get("random_effects_cond_var"),
                1,
                true,
            )?,
        }])
    };
    let Some(random) = fit.get("random").and_then(Zval::array) else {
        return one_intercept("group".into());
    };
    let mut terms = vec![];
    for (i, term) in random.values().enumerate() {
        let term = term
            .array()
            .ok_or_else(|| not_a_fit("random terms must be arrays"))?;
        let group = text(term, "group").unwrap_or_else(|| format!("g{}", i + 1));
        if term.get("covariance").is_none() {
            // A binomial model's single (1 | g) term keeps its variance at the top level.
            return one_intercept(group);
        }
        let covariance = matrix(term.get("covariance"), "covariance")?;
        let k = covariance.len();
        if k == 0 || covariance.iter().any(|row| row.len() != k) {
            return Err(not_a_fit("a random term's covariance must be square"));
        }
        let columns = strings(term.get("columns"))
            .unwrap_or_else(|| (1..=covariance.len()).map(|j| format!("z{j}")).collect());
        if columns.len() != k {
            return Err(not_a_fit(
                "a random term's columns and covariance differ in size",
            ));
        }
        let n_groups = term
            .get("group_labels")
            .and_then(Zval::array)
            .map(|a| a.len())
            .unwrap_or(0);
        let correlated = term.get("correlated").and_then(Zval::bool).unwrap_or(true);
        let values = random_values(
            labels(term.get("group_labels")),
            term.get("effects"),
            term.get("effects_cond_var"),
            k,
            false,
        )?;
        terms.push(RandomTerm {
            group,
            columns,
            covariance,
            correlated,
            n_groups,
            values,
        });
    }
    Ok(terms)
}

/// A fit's printer, built from the fit's keys. Each model kind has one.
type Build = fn(&ZendHashTable) -> PhpResult<Box<dyn Printer>>;

/// One printer per value of a fit's `model` key, which every model function sets. A new model kind adds one entry
/// here and one `Printer` in stats_core::summary; `summary()` and `tidy()` both dispatch through this table.
const PRINTERS: [(&str, Build); 6] = [
    ("linear", linear),
    ("iv", iv),
    ("ridge", ridge),
    ("generalized_linear", generalized_linear),
    ("mixed_gaussian", mixed_gaussian),
    ("mixed_logistic", mixed_logistic),
];

fn printer(fit: &ZendHashTable) -> PhpResult<Box<dyn Printer>> {
    let kind = text(fit, "model").ok_or_else(|| not_a_fit("missing model"))?;
    let (_, build) = PRINTERS
        .iter()
        .find(|(k, _)| *k == kind)
        .ok_or_else(|| not_a_fit(&format!("unknown model '{kind}'")))?;
    build(fit)
}

fn df_residual(fit: &ZendHashTable) -> PhpResult<i64> {
    long(fit, "df_residual").ok_or_else(|| not_a_fit("missing df_residual"))
}

fn n_observations(fit: &ZendHashTable) -> PhpResult<i64> {
    long(fit, "n_observations").ok_or_else(|| not_a_fit("missing n_observations"))
}

fn intercept(fit: &ZendHashTable) -> Option<bool> {
    fit.get("intercept").and_then(Zval::bool)
}

fn linear(fit: &ZendHashTable) -> PhpResult<Box<dyn Printer>> {
    let coefficients = coefficients(fit)?;
    let f_df = intercept(fit).map(|i| coefficients.estimates.len() as i64 - i64::from(i));
    Ok(Box::new(Linear {
        formula: text(fit, "formula"),
        sigma: required_float(fit, "sigma")?,
        df_residual: df_residual(fit)?,
        r_squared: required_float(fit, "r_squared")?,
        adj_r_squared: required_float(fit, "adj_r_squared")?,
        f_df,
        coefficients,
    }))
}

struct RidgePrinter {
    formula: String,
    lambda: f64,
    df: f64,
    df_residual: f64,
    sigma: f64,
    r_squared: f64,
    coefficients: Coefficients,
}
impl Printer for RidgePrinter {
    fn heading(&self) -> String {
        format!(
            "Gaussian ridge regression\nFormula: {}\nLambda: {}\n",
            self.formula,
            summary::format_g(self.lambda, 4)
        )
    }
    fn coefficients(&self) -> &Coefficients {
        &self.coefficients
    }
    fn after_coefficients(&self) -> String {
        format!(
            "\nEffective degrees of freedom: {}\nResidual standard error: {}\nR-squared: {}\nConditional SEs and intervals hold lambda fixed. Estimates are biased; intervals are not coverage-valid.\nAt-bound coefficients have unavailable uncertainty.\n",
            summary::format_g(self.df, 4),
            summary::format_g(self.sigma, 4),
            summary::format_g(self.r_squared, 4)
        )
    }
    fn interval_df(&self) -> Option<f64> {
        Some(self.df_residual)
    }
}
fn ridge(fit: &ZendHashTable) -> PhpResult<Box<dyn Printer>> {
    Ok(Box::new(RidgePrinter {
        formula: text(fit, "formula").unwrap_or_default(),
        lambda: required_float(fit, "lambda")?,
        df: required_float(fit, "df")?,
        df_residual: required_float(fit, "df_residual")?,
        sigma: required_float(fit, "sigma")?,
        r_squared: required_float(fit, "r_squared")?,
        coefficients: coefficients(fit)?,
    }))
}

struct IvPrinter {
    base: Box<dyn Printer>,
    diagnostics: String,
}
impl Printer for IvPrinter {
    fn heading(&self) -> String {
        self.base.heading().replace(
            "Linear model fit by least squares",
            "Instrumental-variable model fit by two-stage least squares",
        )
    }
    fn before_coefficients(&self) -> String {
        self.base.before_coefficients()
    }
    fn coefficients_title(&self) -> &'static str {
        self.base.coefficients_title()
    }
    fn coefficients(&self) -> &summary::Coefficients {
        self.base.coefficients()
    }
    fn after_coefficients(&self) -> String {
        let text = self.base.after_coefficients();
        // OLS's R²-based overall F is not a valid IV Wald test.
        let mut out = text
            .lines()
            .filter(|l| !l.starts_with("F-statistic:"))
            .collect::<Vec<_>>()
            .join("\n");
        out.push('\n');
        out.push_str(&self.diagnostics);
        out
    }
    fn interval_df(&self) -> Option<f64> {
        self.base.interval_df()
    }
}
fn iv(fit: &ZendHashTable) -> PhpResult<Box<dyn Printer>> {
    let mut diagnostics =
        String::from("\nDiagnostic tests (iid covariance):\nTest  df1  df2  statistic  p-value\n");
    let fmt = |v: Option<f64>| v.map_or("NA".into(), |v| summary::format_g(v, 4));
    let mut row = |label: &str,
                   df1: Option<f64>,
                   df2: Option<f64>,
                   statistic: Option<f64>,
                   p: Option<f64>| {
        diagnostics += &format!(
            "{label}  {}  {}  {}  {}\n",
            fmt(df1),
            fmt(df2),
            fmt(statistic),
            fmt(p)
        );
    };
    if let Some(stages) = fit.get("first_stage").and_then(Zval::array) {
        for (name, value) in stages.iter() {
            if let Some(stage) = value.array() {
                row(
                    &format!("Weak instruments ({name})"),
                    float(stage, "df1"),
                    float(stage, "df2"),
                    float(stage, "f_statistic"),
                    float(stage, "p_value"),
                );
                row(
                    &format!("Sanderson-Windmeijer ({name})"),
                    float(stage, "sw_df1"),
                    float(stage, "sw_df2"),
                    float(stage, "sw_f_statistic"),
                    float(stage, "sw_p_value"),
                );
            }
        }
    }
    if let Some(d) = fit.get("diagnostics").and_then(Zval::array) {
        if let Some(t) = d.get("wu_hausman").and_then(Zval::array) {
            row(
                "Wu-Hausman",
                float(t, "df1"),
                float(t, "df2"),
                float(t, "statistic"),
                float(t, "p_value"),
            );
        }
        if let Some(t) = d.get("sargan").and_then(Zval::array) {
            row(
                "Sargan",
                float(t, "df"),
                None,
                float(t, "statistic"),
                float(t, "p_value"),
            );
        }
    }
    Ok(Box::new(IvPrinter {
        base: linear(fit)?,
        diagnostics,
    }))
}

fn generalized_linear(fit: &ZendHashTable) -> PhpResult<Box<dyn Printer>> {
    let coefficients = coefficients(fit)?;
    let n = n_observations(fit)?;
    Ok(Box::new(GeneralizedLinear {
        family: text(fit, "family").zip(text(fit, "link")),
        formula: text(fit, "formula"),
        // A Gaussian GLM estimates its dispersion and reports t statistics; the others fix it at one.
        dispersion: if coefficients.statistic == 't' {
            required_float(fit, "residual_variance")?
        } else {
            1.0
        },
        null_deviance: required_float(fit, "null_deviance")?,
        deviance: required_float(fit, "deviance")?,
        df_null: intercept(fit).map(|i| n - i64::from(i)),
        df_residual: df_residual(fit)?,
        aic: required_float(fit, "aic")?,
        coefficients,
    }))
}

fn mixed_fit(fit: &ZendHashTable) -> PhpResult<MixedFit> {
    Ok(MixedFit {
        log_likelihood: required_float(fit, "log_likelihood")?,
        aic: float(fit, "aic"),
        bic: float(fit, "bic"),
        df_residual: df_residual(fit)?,
        n_observations: n_observations(fit)?,
        terms: random_terms(fit)?,
        singular: fit
            .get("singular")
            .or_else(|| fit.get("boundary"))
            .and_then(Zval::bool)
            .unwrap_or(false),
    })
}

fn mixed_gaussian(fit: &ZendHashTable) -> PhpResult<Box<dyn Printer>> {
    Ok(Box::new(MixedGaussian {
        formula: text(fit, "formula"),
        reml: text(fit, "method").as_deref() == Some("REML"),
        fit: mixed_fit(fit)?,
        residual_variance: required_float(fit, "residual_variance")?,
        coefficients: coefficients(fit)?,
    }))
}

fn mixed_logistic(fit: &ZendHashTable) -> PhpResult<Box<dyn Printer>> {
    let integration = match text(fit, "integration").as_deref() {
        Some("laplace") => "Laplace approximation".to_string(),
        Some(_) => format!(
            "adaptive Gauss-Hermite quadrature, {} points",
            long(fit, "quadrature_points").unwrap_or(0)
        ),
        None => return Err(not_a_fit("missing integration")),
    };
    Ok(Box::new(MixedLogistic {
        formula: text(fit, "formula"),
        integration,
        fit: mixed_fit(fit)?,
        coefficients: coefficients(fit)?,
    }))
}

/// The text of `Phit\\summary()` for a fit's result array.
pub fn summary_text(fit: &ZendHashTable) -> PhpResult<String> {
    Ok(summary::render(printer(fit)?.as_ref()))
}

/// The result array behind a `Fit|array` argument.
fn fit_argument(fit: &Zval) -> PhpResult<&ZendHashTable> {
    crate::fit::fit_data(fit).ok_or_else(|| not_a_fit("not a fit object or array"))
}

#[php_function]
#[php(name = "Phit\\summary")]
pub fn summary(fit: &Zval) -> PhpResult<String> {
    summary_text(fit_argument(fit)?)
}

/// A `Phit\\Table`'s text: its columns in the layout of R's print.data.frame().
pub fn table_text(columns: &ZendHashTable) -> PhpResult<String> {
    let mut names = vec![];
    let mut cells = vec![];
    for (key, column) in columns.iter() {
        names.push(key.to_string());
        let values = column
            .array()
            .ok_or_else(|| invalid("table columns must be arrays"))?;
        let text = values.values().all(|v| v.is_null() || v.is_string());
        cells.push(if text {
            summary::Cells::Text(
                values
                    .values()
                    .map(|v| v.str().map(str::to_owned))
                    .collect(),
            )
        } else {
            summary::Cells::Numbers(
                values
                    .values()
                    .map(|v| v.double().or_else(|| v.long().map(|l| l as f64)))
                    .collect(),
            )
        });
    }
    Ok(summary::data_frame(&names, &cells))
}

fn column<T: ext_php_rs::convert::IntoZval + Clone>(
    values: &[T],
) -> PhpResult<ZBox<ZendHashTable>> {
    let mut column = ZendHashTable::new();
    for v in values {
        column.push(v.clone())?;
    }
    Ok(column)
}

/// The fixed effects, as broom's `tidy()`.
fn tidy_fixed(printer: &dyn Printer, confInt: bool, level: f64) -> PhpResult<ZBox<ZendHashTable>> {
    let c = printer.coefficients();
    let mut out = ZendHashTable::new();
    out.insert("term", column(&c.names)?)?;
    out.insert("estimate", column(&c.estimates)?)?;
    out.insert("std_error", column(&c.std_errors)?)?;
    out.insert("statistic", column(&c.statistics)?)?;
    out.insert("p_value", column(&c.p_values)?)?;
    if confInt {
        let intervals = summary::confidence_intervals(
            &c.estimates,
            &c.std_errors,
            printer.interval_df(),
            level,
        );
        let (low, high): (Vec<f64>, Vec<f64>) = intervals.into_iter().unzip();
        out.insert("conf_low", column(&low)?)?;
        out.insert("conf_high", column(&high)?)?;
    }
    Ok(out)
}

/// Each group's conditional modes and standard errors, as broom.mixed's `tidy(effects = "ran_vals")`: one row per
/// term, column, and group, in that order. Intervals use the normal distribution.
fn tidy_random(printer: &dyn Printer, confInt: bool, level: f64) -> PhpResult<ZBox<ZendHashTable>> {
    let terms = printer
        .random_terms()
        .ok_or_else(|| invalid("effects 'ran_vals' needs a mixed model fit"))?;
    let (mut group, mut levels, mut term) = (vec![], vec![], vec![]);
    let (mut estimate, mut std_error) = (vec![], vec![]);
    for t in terms {
        let values = t
            .values
            .as_ref()
            .ok_or_else(|| not_a_fit("missing random effects"))?;
        for (j, name) in t.columns.iter().enumerate() {
            for (g, label) in values.levels.iter().enumerate() {
                group.push(t.group.clone());
                levels.push(label.clone());
                term.push(name.clone());
                estimate.push(values.estimates[g][j]);
                std_error.push(values.std_errors[g][j]);
            }
        }
    }
    let mut out = ZendHashTable::new();
    out.insert("group", column(&group)?)?;
    out.insert("level", column(&levels)?)?;
    out.insert("term", column(&term)?)?;
    out.insert("estimate", column(&estimate)?)?;
    out.insert("std_error", column(&std_error)?)?;
    if confInt {
        let (low, high): (Vec<f64>, Vec<f64>) =
            summary::confidence_intervals(&estimate, &std_error, None, level)
                .into_iter()
                .unzip();
        out.insert("conf_low", column(&low)?)?;
        out.insert("conf_high", column(&high)?)?;
    }
    Ok(out)
}

/// `tidy()`'s columns for a fit's result array.
#[allow(non_snake_case)] // PHP parameter names are camelCase.
pub fn tidy_columns(
    fit: &ZendHashTable,
    confInt: bool,
    level: f64,
    effects: &str,
) -> PhpResult<ZBox<ZendHashTable>> {
    if !(level > 0.0 && level < 1.0) {
        return Err(invalid("level must be in (0, 1)"));
    }
    if !matches!(effects, "fixed" | "ran_vals") {
        return Err(invalid("effects must be 'fixed' or 'ran_vals'"));
    }
    let printer = printer(fit)?;
    if effects == "fixed" {
        let mut out = tidy_fixed(printer.as_ref(), confInt, level)?;
        if text(fit, "model").as_deref() == Some("ridge") {
            for key in ["std_error", "statistic", "p_value", "conf_low", "conf_high"] {
                if let Some(a) = out.get(key).and_then(Zval::array) {
                    let values = a
                        .values()
                        .map(|v| v.double().filter(|v| !v.is_nan()))
                        .collect::<Vec<_>>();
                    out.insert(key, values)?;
                }
            }
        }
        Ok(out)
    } else {
        tidy_random(printer.as_ref(), confInt, level)
    }
}

#[php_function]
#[php(
    name = "Phit\\tidy",
    defaults(confInt = false, level = 0.95, effects = "\"fixed\"")
)]
#[allow(non_snake_case)] // PHP parameter names are camelCase.
pub fn tidy(fit: &Zval, confInt: bool, level: f64, effects: &str) -> PhpResult<Zval> {
    if !(level > 0.0 && level < 1.0) {
        return Err(invalid("level must be in (0, 1)"));
    }
    if !matches!(effects, "fixed" | "ran_vals") {
        return Err(invalid("effects must be 'fixed' or 'ran_vals'"));
    }
    crate::fit::into_table(tidy_columns(fit_argument(fit)?, confInt, level, effects)?)
}

pub fn register(module: ModuleBuilder) -> ModuleBuilder {
    module
        .function(wrap_function!(summary))
        .function(wrap_function!(tidy))
}
