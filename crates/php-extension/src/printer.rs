//! PHP bindings for model summaries: Phit\summary() prints a fit in the layout of R's print(summary(fit)), and
//! Phit\tidy() returns its coefficient table as columns, as broom's tidy() does. Both pick the fit's printer from
//! its `model` key through PRINTERS; the layouts are the stats_core::summary::Printer implementations.
use crate::stats_exception;
use ext_php_rs::{boxed::ZBox, prelude::*, types::ZendHashTable, types::Zval};
use stats_core::summary::{
    self, Coefficients, GeneralizedLinear, Linear, MixedFit, MixedGaussian, MixedLogistic, Printer,
    RandomTerm,
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

fn random_terms(fit: &ZendHashTable) -> PhpResult<Vec<RandomTerm>> {
    let one_intercept = |group: String| -> PhpResult<Vec<RandomTerm>> {
        Ok(vec![RandomTerm {
            group,
            columns: vec!["(Intercept)".into()],
            covariance: vec![vec![required_float(fit, "random_intercept_variance")?]],
            correlated: false,
            n_groups: long(fit, "n_groups").ok_or_else(|| not_a_fit("missing n_groups"))? as usize,
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
        terms.push(RandomTerm {
            group,
            columns,
            covariance,
            correlated,
            n_groups,
        });
    }
    Ok(terms)
}

/// A fit's printer, built from the fit's keys. Each model kind has one.
type Build = fn(&ZendHashTable) -> PhpResult<Box<dyn Printer>>;

/// One printer per value of a fit's `model` key, which every model function sets. A new model kind adds one entry
/// here and one `Printer` in stats_core::summary; `summary()` and `tidy()` both dispatch through this table.
const PRINTERS: [(&str, Build); 4] = [
    ("linear", linear),
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

#[php_function]
#[php(name = "Phit\\summary")]
pub fn summary(fit: &ZendHashTable) -> PhpResult<String> {
    Ok(summary::render(printer(fit)?.as_ref()))
}

#[php_function]
#[php(name = "Phit\\tidy", defaults(confInt = false, level = 0.95))]
#[allow(non_snake_case)] // PHP parameter names are camelCase.
pub fn tidy(fit: &ZendHashTable, confInt: bool, level: f64) -> PhpResult<ZBox<ZendHashTable>> {
    if !(level > 0.0 && level < 1.0) {
        return Err(invalid("level must be in (0, 1)"));
    }
    let printer = printer(fit)?;
    let c = printer.coefficients();
    let mut out = ZendHashTable::new();
    let mut term = ZendHashTable::new();
    for name in &c.names {
        term.push(name.as_str())?;
    }
    out.insert("term", term)?;
    for (key, values) in [
        ("estimate", &c.estimates),
        ("std_error", &c.std_errors),
        ("statistic", &c.statistics),
        ("p_value", &c.p_values),
    ] {
        let mut column = ZendHashTable::new();
        for v in values {
            column.push(*v)?;
        }
        out.insert(key, column)?;
    }
    if confInt {
        let intervals = summary::confidence_intervals(
            &c.estimates,
            &c.std_errors,
            printer.interval_df(),
            level,
        );
        let (mut low, mut high) = (ZendHashTable::new(), ZendHashTable::new());
        for (l, h) in intervals {
            low.push(l)?;
            high.push(h)?;
        }
        out.insert("conf_low", low)?;
        out.insert("conf_high", high)?;
    }
    Ok(out)
}

pub fn register(module: ModuleBuilder) -> ModuleBuilder {
    module
        .function(wrap_function!(summary))
        .function(wrap_function!(tidy))
}
