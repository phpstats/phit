//! Formula interfaces: model_matrix, fit_lm, fit_glm and fit_lmer.
//!
//! Data arrive as named columns (or an object with toModelData()/toColumns()).
//! The R-compatible formula engine lives in stats_core::formula; this module only
//! converts PHP values, calls the same estimation cores as the matrix functions
//! and adds coefficient names to their unchanged result contracts.
use crate::{finish_result, input_exception, stats_exception};
use ext_php_rs::{
    boxed::ZBox,
    prelude::*,
    types::{ZendHashTable, Zval},
    zend::ExecutorGlobals,
};
use stats_core::ErrorKind;
use stats_core::formula::{
    Column, Data, DesignOptions, GroupLabel, ModelDesign, PolyCoefs, Predvars, Response,
};

fn invalid(message: &str) -> PhpException {
    input_exception(ErrorKind::InvalidInput, message)
}

fn factor_descriptor(name: &str, table: &ZendHashTable) -> PhpResult<Column> {
    let malformed = |detail: &str| {
        invalid(&format!(
            "column '{name}' is not a valid factor descriptor (exactly levels: list<string>, codes: list<?int>, ordered: bool): {detail}"
        ))
    };
    if table.len() != 3 {
        return Err(malformed("unexpected keys"));
    }
    let levels = table
        .get("levels")
        .and_then(|v| v.array())
        .ok_or_else(|| malformed("levels must be an array"))?
        .values()
        .map(|v| v.str().map(str::to_owned))
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| malformed("levels must be strings"))?;
    let codes = table
        .get("codes")
        .and_then(|v| v.array())
        .ok_or_else(|| malformed("codes must be an array"))?
        .values()
        .map(|v| {
            if v.is_null() {
                Ok(None)
            } else {
                match v.long() {
                    Some(c) if c >= 0 && (c as usize) < levels.len() => Ok(Some(c as usize)),
                    Some(_) => Err(malformed("codes must be 0-based indexes into levels")),
                    None => Err(malformed("codes must be integers or null")),
                }
            }
        })
        .collect::<PhpResult<Vec<_>>>()?;
    let ordered = table
        .get("ordered")
        .and_then(|v| v.bool())
        .ok_or_else(|| malformed("ordered must be a boolean"))?;
    Ok(Column::Factor {
        levels,
        codes,
        ordered,
    })
}

fn column(name: &str, value: &Zval) -> PhpResult<Column> {
    let table = value.array().ok_or_else(|| {
        invalid(&format!(
            "column '{name}' must be an array of values or a factor descriptor"
        ))
    })?;
    if !table.has_numerical_keys() {
        return factor_descriptor(name, table);
    }
    let (mut numbers, mut bools, mut strings, mut nested) = (false, false, false, false);
    for v in table.values() {
        if v.is_null() {
        } else if v.is_long() || v.is_double() {
            if v.double().is_some_and(f64::is_infinite) {
                return Err(input_exception(
                    ErrorKind::Nonfinite,
                    &format!("column '{name}' contains an infinite value"),
                ));
            }
            numbers = true;
        } else if v.is_bool() {
            bools = true;
        } else if v.is_string() {
            strings = true;
        } else {
            nested = true;
        }
    }
    let len = table.len();
    if nested {
        return Ok(Column::Invalid {
            len,
            message: "list-columns (arrays or objects as values) cannot be used in a model".into(),
        });
    }
    if u8::from(numbers) + u8::from(bools) + u8::from(strings) > 1 {
        return Ok(Column::Invalid {
            len,
            message: "values mix numbers, booleans and strings".into(),
        });
    }
    Ok(if strings {
        Column::Text(table.values().map(|v| v.str().map(str::to_owned)).collect())
    } else if bools {
        Column::Logical(
            table
                .values()
                .map(|v| v.bool().filter(|_| !v.is_null()))
                .collect(),
        )
    } else {
        Column::Numeric(
            table
                .values()
                .map(|v| {
                    v.double()
                        .or_else(|| v.long().map(|l| l as f64))
                        .filter(|x| !x.is_nan())
                })
                .collect(),
        )
    })
}

fn columns_from(table: &ZendHashTable) -> PhpResult<Data> {
    let mut columns = Vec::with_capacity(table.len());
    for (key, value) in table.iter() {
        let name = key.to_string();
        if !matches!(
            key,
            ext_php_rs::types::ArrayKey::String(_) | ext_php_rs::types::ArrayKey::Str(_)
        ) {
            return Err(invalid(
                "data must be an associative array of columns (name => list of values), not a list of rows",
            ));
        }
        columns.push((name.clone(), column(&name, value)?));
    }
    Data::new(columns).map_err(stats_exception)
}

fn model_data(data: &Zval) -> PhpResult<Data> {
    if let Some(table) = data.array() {
        return columns_from(table);
    }
    if let Some(object) = data.object() {
        for method in ["toModelData", "toColumns"] {
            match object.try_call_method(method, vec![]) {
                Ok(result) => {
                    ExecutorGlobals::throw_if_exception()?;
                    let table = result.array().ok_or_else(|| {
                        invalid(&format!("{method}() must return an array of columns"))
                    })?;
                    return columns_from(table);
                }
                Err(ext_php_rs::error::Error::Callable) => continue,
                Err(e) => return Err(PhpException::from(e.to_string())),
            }
        }
    }
    Err(invalid(
        "data must be an array of columns or an object with toModelData() or toColumns()",
    ))
}

fn missing_policy(missing: &str) -> PhpResult<bool> {
    match missing {
        "drop" => Ok(true),
        "raise" => Ok(false),
        _ => Err(invalid("missing must be 'drop' or 'raise'")),
    }
}

fn weights_vector(weights: Option<&Zval>, data: &Data) -> PhpResult<Option<Vec<Option<f64>>>> {
    let Some(weights) = weights.filter(|w| !w.is_null()) else {
        return Ok(None);
    };
    let values: Vec<Option<f64>> = if let Some(name) = weights.str() {
        match data.column(name) {
            Some(Column::Numeric(v)) => v.clone(),
            Some(_) => return Err(invalid(&format!("weights column '{name}' must be numeric"))),
            None => return Err(invalid(&format!("unknown weights column '{name}'"))),
        }
    } else if let Some(table) = weights.array() {
        table
            .values()
            .map(crate::number)
            .collect::<PhpResult<Vec<_>>>()?
    } else {
        return Err(invalid("weights must be null, an array or a column name"));
    };
    if values.len() != data.rows() {
        return Err(input_exception(
            ErrorKind::DimensionMismatch,
            "optional vectors must have one value per original observation",
        ));
    }
    if values.iter().flatten().any(|w| *w <= 0.0) {
        return Err(invalid("weights must be positive"));
    }
    Ok(Some(values))
}

struct Prepared {
    design: ModelDesign,
    weights: Vec<f64>,
}

fn prepare(
    formula: &str,
    data: &Zval,
    missing: &str,
    weights: Option<&Zval>,
    drop_unused_levels: bool,
) -> PhpResult<Prepared> {
    let drop_missing = missing_policy(missing)?;
    let frame = model_data(data)?;
    let weights = weights_vector(weights, &frame)?;
    let options = DesignOptions {
        drop_missing,
        drop_unused_levels,
        extra: weights.into_iter().collect(),
    };
    let design =
        stats_core::formula::model_design(formula, &frame, &options).map_err(stats_exception)?;
    let weights = design
        .extra
        .first()
        .cloned()
        .unwrap_or_else(|| vec![1.0; design.x.len()]);
    Ok(Prepared { design, weights })
}

fn group_values(groups: &[GroupLabel]) -> PhpResult<ZBox<ZendHashTable>> {
    let mut out = ZendHashTable::new();
    for g in groups {
        match g {
            GroupLabel::Integer(v) => out.push(*v)?,
            GroupLabel::Text(v) => out.push(v.clone())?,
        }
    }
    Ok(out)
}

fn rows(design: &ModelDesign) -> (Vec<i64>, Vec<i64>) {
    (
        design.retained_rows.iter().map(|v| *v as i64).collect(),
        design.dropped_rows.iter().map(|v| *v as i64).collect(),
    )
}

fn random_table(design: &ModelDesign) -> PhpResult<ZBox<ZendHashTable>> {
    let mut random = ZendHashTable::new();
    for term in &design.random {
        let mut t = ZendHashTable::new();
        t.insert("label", term.label.clone())?;
        t.insert("group", term.group.clone())?;
        t.insert("columns", term.columns.clone())?;
        t.insert("correlated", term.correlated)?;
        t.insert("z", term.z.clone())?;
        t.insert("groups", group_values(&term.groups)?)?;
        random.push(t)?;
    }
    Ok(random)
}

/// Shared naming keys added to every formula fit.
fn add_names(output: &mut ZendHashTable, design: &ModelDesign) -> PhpResult<()> {
    output.insert("formula", design.formula.clone())?;
    output.insert("response", design.response_name.clone())?;
    output.insert("columns", design.columns.clone())?;
    output.insert("terms", design.terms.clone())?;
    output.insert(
        "assign",
        design.assign.iter().map(|v| *v as i64).collect::<Vec<_>>(),
    )?;
    output.insert("intercept", design.intercept)?;
    let mut named = ZendHashTable::new();
    if let Some(coefficients) = output.get("coefficients").and_then(|v| v.array()) {
        for (name, value) in design.columns.iter().zip(coefficients.values()) {
            named.insert(name.as_str(), value.shallow_clone())?;
        }
    }
    output.insert("named", named)?;
    output.insert("predvars", predvars_table(&design.predvars)?)?;
    Ok(())
}

/// Factor levels and poly() coefficients that predict() reuses.
fn predvars_table(predvars: &Predvars) -> PhpResult<ZBox<ZendHashTable>> {
    let mut levels = ZendHashTable::new();
    for (name, values) in &predvars.levels {
        levels.insert(name.as_str(), values.clone())?;
    }
    let mut poly = ZendHashTable::new();
    for (text, coefs) in &predvars.poly {
        let mut c = ZendHashTable::new();
        c.insert("alpha", coefs.alpha.clone())?;
        c.insert("norm2", coefs.norm2.clone())?;
        poly.insert(text.as_str(), c)?;
    }
    let mut out = ZendHashTable::new();
    out.insert("levels", levels)?;
    out.insert("poly", poly)?;
    Ok(out)
}

fn malformed_fit(detail: &str) -> PhpException {
    invalid(&format!(
        "fit must be a result of Phit\\fit_lm, Phit\\fit_glm, Phit\\fit_lmer or Phit\\fit_glmer: {detail}"
    ))
}

fn float_list(value: Option<&Zval>, what: &str) -> PhpResult<Vec<f64>> {
    value
        .and_then(|v| v.array())
        .ok_or_else(|| malformed_fit(&format!("missing {what}")))?
        .values()
        .map(|v| {
            v.double()
                .or_else(|| v.long().map(|l| l as f64))
                .ok_or_else(|| malformed_fit(&format!("{what} must be numbers")))
        })
        .collect()
}

fn string_list(value: Option<&Zval>, what: &str) -> PhpResult<Vec<String>> {
    value
        .and_then(|v| v.array())
        .ok_or_else(|| malformed_fit(&format!("missing {what}")))?
        .values()
        .map(|v| {
            v.str()
                .map(str::to_owned)
                .ok_or_else(|| malformed_fit(&format!("{what} must be strings")))
        })
        .collect()
}

fn predvars_from(fit: &ZendHashTable) -> PhpResult<Predvars> {
    let table = fit
        .get("predvars")
        .and_then(|v| v.array())
        .ok_or_else(|| malformed_fit("missing predvars"))?;
    let mut predvars = Predvars::default();
    if let Some(levels) = table.get("levels").and_then(|v| v.array()) {
        for (name, values) in levels.iter() {
            predvars
                .levels
                .push((name.to_string(), string_list(Some(values), "levels")?));
        }
    }
    if let Some(poly) = table.get("poly").and_then(|v| v.array()) {
        for (text, coefs) in poly.iter() {
            let coefs = coefs
                .array()
                .ok_or_else(|| malformed_fit("poly coefficients must be arrays"))?;
            predvars.poly.push((
                text.to_string(),
                PolyCoefs {
                    alpha: float_list(coefs.get("alpha"), "poly alpha")?,
                    norm2: float_list(coefs.get("norm2"), "poly norm2")?,
                },
            ));
        }
    }
    Ok(predvars)
}

fn label_key(value: &Zval) -> Option<String> {
    value
        .long()
        .map(|v| v.to_string())
        .or_else(|| value.str().map(str::to_owned))
}

fn design_label_key(label: &GroupLabel) -> String {
    match label {
        GroupLabel::Integer(v) => v.to_string(),
        GroupLabel::Text(v) => v.clone(),
    }
}

/// Fitted random effects by term: group label -> coefficient vector.
fn fitted_effects(
    fit: &ZendHashTable,
) -> PhpResult<Vec<std::collections::HashMap<String, Vec<f64>>>> {
    let malformed = || malformed_fit("random effects are incomplete");
    let random = fit
        .get("random")
        .and_then(|v| v.array())
        .ok_or_else(malformed)?;
    let mut out = Vec::new();
    if let Some(effects) = fit.get("random_effects").and_then(|v| v.array()) {
        // fit_glmer: one random intercept with top-level effects and labels.
        let labels = fit
            .get("group_labels")
            .and_then(|v| v.array())
            .ok_or_else(malformed)?;
        let mut map = std::collections::HashMap::new();
        for (label, effect) in labels.values().zip(effects.values()) {
            let value = effect.double().ok_or_else(malformed)?;
            map.insert(label_key(label).ok_or_else(malformed)?, vec![value]);
        }
        out.push(map);
        return Ok(out);
    }
    for term in random.values() {
        let term = term.array().ok_or_else(malformed)?;
        let labels = term
            .get("group_labels")
            .and_then(|v| v.array())
            .ok_or_else(malformed)?;
        let effects = term
            .get("effects")
            .and_then(|v| v.array())
            .ok_or_else(malformed)?;
        let mut map = std::collections::HashMap::new();
        for (label, effect) in labels.values().zip(effects.values()) {
            map.insert(
                label_key(label).ok_or_else(malformed)?,
                float_list(Some(effect), "random effects")?,
            );
        }
        out.push(map);
    }
    Ok(out)
}

fn quad_form(x: &[f64], covariance: &[Vec<f64>]) -> f64 {
    let mut total = 0.0;
    for (i, xi) in x.iter().enumerate() {
        for (j, xj) in x.iter().enumerate() {
            total += xi * covariance[i][j] * xj;
        }
    }
    total
}

fn nullable(values: &[Option<f64>]) -> PhpResult<ZBox<ZendHashTable>> {
    let mut out = ZendHashTable::new();
    for v in values {
        match v {
            Some(v) => out.push(*v)?,
            None => out.push(Option::<f64>::None)?,
        }
    }
    Ok(out)
}

/// Predictions for new data from a formula fit, as R's predict() for lm, glm
/// and merMod objects.
#[php_function]
#[php(
    name = "Phit\\predict",
    defaults(
        r#type = "\"link\"",
        seFit = false,
        interval = "\"none\"",
        level = 0.95,
        random = true,
        allowNewLevels = false
    )
)]
#[allow(clippy::too_many_arguments)] // Named PHP arguments mirror R's predict().
pub fn predict(
    fit: &ZendHashTable,
    newdata: &Zval,
    r#type: &str,
    seFit: bool,
    interval: &str,
    level: f64,
    random: bool,
    allowNewLevels: bool,
) -> PhpResult<ZBox<ZendHashTable>> {
    let formula = fit
        .get("formula")
        .and_then(|v| v.str())
        .ok_or_else(|| malformed_fit("missing formula"))?
        .to_owned();
    let mixed = fit.get("random").is_some();
    let link = match fit.get("link").and_then(|v| v.str()) {
        Some(l @ ("identity" | "logit" | "probit" | "log")) => l.to_owned(),
        Some(_) => return Err(malformed_fit("unknown link")),
        None => "identity".to_owned(),
    };
    let is_lm = fit.get("family").is_none() && !mixed;
    let response = match r#type {
        "link" => false,
        "response" => true,
        _ => return Err(invalid("type must be 'link' or 'response'")),
    };
    let interval_kind = match interval {
        "none" => None,
        "confidence" | "prediction" if is_lm => Some(interval == "prediction"),
        "confidence" | "prediction" => {
            return Err(input_exception(
                ErrorKind::Unsupported,
                "intervals are available for Phit\\fit_lm fits; use seFit for GLMs",
            ));
        }
        _ => {
            return Err(invalid(
                "interval must be 'none', 'confidence' or 'prediction'",
            ));
        }
    };
    if !(level > 0.0 && level < 1.0) {
        return Err(invalid("level must be strictly between 0 and 1"));
    }
    if mixed && (seFit || interval_kind.is_some()) {
        return Err(input_exception(
            ErrorKind::Unsupported,
            "standard errors and intervals are not available for mixed-model predictions",
        ));
    }
    let coefficients = float_list(fit.get("coefficients"), "coefficients")?;
    let columns = string_list(fit.get("columns"), "columns")?;
    let covariance: Vec<Vec<f64>> = fit
        .get("covariance")
        .and_then(|v| v.array())
        .ok_or_else(|| malformed_fit("missing covariance"))?
        .values()
        .map(|row| float_list(Some(row), "covariance"))
        .collect::<PhpResult<_>>()?;
    if coefficients.len() != columns.len()
        || covariance.len() != columns.len()
        || covariance.iter().any(|r| r.len() != columns.len())
    {
        return Err(malformed_fit(
            "coefficients, columns and covariance disagree",
        ));
    }
    let predvars = predvars_from(fit)?;
    let frame = model_data(newdata)?;
    let design =
        stats_core::formula::prediction_design(&formula, &frame, &predvars, mixed && random)
            .map_err(stats_exception)?;
    if design.columns != columns {
        return Err(invalid(&format!(
            "newdata produce columns [{}] but the fit has [{}]",
            design.columns.join(", "),
            columns.join(", ")
        )));
    }
    let effects = if mixed && random {
        Some(fitted_effects(fit)?)
    } else {
        None
    };
    let n = frame.rows();
    let mut eta = vec![None; n];
    let mut se = vec![None; n];
    let mut new_levels = Vec::new();
    for (k, row) in design.retained_rows.iter().enumerate() {
        let x = &design.x[k];
        let mut value: f64 = x.iter().zip(&coefficients).map(|(a, b)| a * b).sum();
        if let Some(offset) = &design.offset {
            value += offset[k];
        }
        if let Some(effects) = &effects {
            for (term, fitted) in design.random.iter().zip(effects) {
                let key = design_label_key(&term.groups[k]);
                match fitted.get(&key) {
                    Some(b) => {
                        value += term.z[k].iter().zip(b).map(|(z, b)| z * b).sum::<f64>();
                    }
                    None if allowNewLevels => {}
                    None => {
                        let label = format!("{}={key}", term.group);
                        if !new_levels.contains(&label) {
                            new_levels.push(label);
                        }
                    }
                }
            }
        }
        eta[*row] = Some(value);
        if seFit || interval_kind.is_some() {
            se[*row] = Some(quad_form(x, &covariance).max(0.0).sqrt());
        }
    }
    if !new_levels.is_empty() {
        return Err(invalid(&format!(
            "new levels detected in newdata: {}; pass allowNewLevels: true to predict them at the population level",
            new_levels.join(", ")
        )));
    }
    let normal = stats_core::distributions::Normal::new(0.0, 1.0).map_err(stats_exception)?;
    let inverse_link = |eta: f64| -> f64 {
        match link.as_str() {
            "logit" => {
                if eta >= 0.0 {
                    1.0 / (1.0 + (-eta).exp())
                } else {
                    let e = eta.exp();
                    e / (1.0 + e)
                }
            }
            "probit" => normal.cdf(eta),
            "log" => eta.exp(),
            _ => eta,
        }
    };
    let derivative = |eta: f64| -> f64 {
        match link.as_str() {
            "logit" => {
                let mu = inverse_link(eta);
                mu * (1.0 - mu)
            }
            "probit" => normal.pdf(eta),
            "log" => eta.exp(),
            _ => 1.0,
        }
    };
    let mut fitted = eta.clone();
    if response {
        for (f, s) in fitted.iter_mut().zip(se.iter_mut()) {
            if let Some(e) = *f {
                *f = Some(inverse_link(e));
                if let Some(v) = *s {
                    *s = Some(v * derivative(e).abs());
                }
            }
        }
    }
    let mut out = ZendHashTable::new();
    out.insert("fit", nullable(&fitted)?)?;
    if seFit {
        out.insert("se_fit", nullable(&se)?)?;
    }
    let df = fit.get("df_residual").and_then(|v| v.long());
    if is_lm && (seFit || interval_kind.is_some()) {
        out.insert("df", df)?;
    }
    if !mixed && (seFit || interval_kind.is_some()) {
        // R's residual.scale: sigma for Gaussian fits, 1 for binomial and Poisson.
        let sigma = match link.as_str() {
            "identity" => fit.get("sigma").and_then(|v| v.double()),
            _ => Some(1.0),
        };
        out.insert("residual_scale", sigma)?;
    }
    if let Some(prediction) = interval_kind {
        let df = df.ok_or_else(|| malformed_fit("missing df_residual"))?;
        let sigma = fit
            .get("sigma")
            .and_then(|v| v.double())
            .ok_or_else(|| malformed_fit("missing sigma"))?;
        let critical = stats_core::distributions::StudentT::new(df as f64)
            .map_err(stats_exception)?
            .quantile((1.0 + level) / 2.0);
        if prediction
            && float_list(fit.get("weights"), "weights").is_ok_and(|w| w.iter().any(|w| *w != 1.0))
        {
            ext_php_rs::error::php_error(
                &ext_php_rs::flags::ErrorType::Warning,
                "Phit\\predict assumes constant prediction variance even though the model fit is weighted",
            );
        }
        let mut lower = vec![None; n];
        let mut upper = vec![None; n];
        for i in 0..n {
            if let (Some(f), Some(s)) = (fitted[i], se[i]) {
                let width = critical
                    * if prediction {
                        (s * s + sigma * sigma).sqrt()
                    } else {
                        s
                    };
                lower[i] = Some(f - width);
                upper[i] = Some(f + width);
            }
        }
        out.insert("lower", nullable(&lower)?)?;
        out.insert("upper", nullable(&upper)?)?;
    }
    out.insert(
        "level",
        if interval_kind.is_some() {
            Some(level)
        } else {
            None
        },
    )?;
    out.insert("n_input", n)?;
    out.insert("n_predicted", design.retained_rows.len())?;
    out.insert(
        "missing_rows",
        design
            .dropped_rows
            .iter()
            .map(|v| *v as i64)
            .collect::<Vec<_>>(),
    )?;
    Ok(out)
}

fn reject_random(design: &ModelDesign, function: &str) -> PhpResult<()> {
    if design.random.is_empty() {
        Ok(())
    } else {
        Err(input_exception(
            ErrorKind::Unsupported,
            &format!(
                "{function} does not fit random-effect terms such as ({}); use Phit\\fit_lmer, or Phit\\fit_mixed with Phit\\model_matrix()['random']",
                design.random[0].label
            ),
        ))
    }
}

fn numeric_response(design: &ModelDesign, function: &str) -> PhpResult<Vec<f64>> {
    match &design.response {
        Some(Response::Numeric(v)) => Ok(v.clone()),
        None => Err(invalid(&format!("{function} needs a response: 'y ~ ...'"))),
        Some(_) => Err(invalid(&format!(
            "{function} needs a numeric or logical response"
        ))),
    }
}

/// Build R's model.matrix() for a formula over named columns.
#[php_function]
#[php(name = "Phit\\model_matrix", defaults(missing = "\"drop\""))]
pub fn model_matrix(formula: &str, data: &Zval, missing: &str) -> PhpResult<ZBox<ZendHashTable>> {
    let prepared = prepare(formula, data, missing, None, false)?;
    let design = prepared.design;
    let mut out = ZendHashTable::new();
    match &design.response {
        None => out.insert("y", Option::<Vec<f64>>::None)?,
        Some(Response::Numeric(v)) => out.insert("y", v.clone())?,
        Some(Response::Factor { levels, codes }) => {
            out.insert("y", codes.iter().map(|c| *c as f64).collect::<Vec<_>>())?;
            out.insert("response_levels", levels.clone())?;
        }
        Some(Response::Binomial {
            successes,
            failures,
        }) => out.insert(
            "y",
            successes
                .iter()
                .zip(failures)
                .map(|(s, f)| vec![*s, *f])
                .collect::<Vec<_>>(),
        )?,
    }
    out.insert("x", design.x.clone())?;
    out.insert("columns", design.columns.clone())?;
    out.insert("intercept", design.intercept)?;
    out.insert("terms", design.terms.clone())?;
    out.insert(
        "assign",
        design.assign.iter().map(|v| *v as i64).collect::<Vec<_>>(),
    )?;
    out.insert("formula", design.formula.clone())?;
    out.insert("response", design.response_name.clone())?;
    out.insert("offset", design.offset.clone())?;
    out.insert("random", random_table(&design)?)?;
    let (retained, dropped) = rows(&design);
    out.insert("n_input", design.n_input)?;
    out.insert("n_observations", retained.len())?;
    out.insert("n_dropped", dropped.len())?;
    out.insert("retained_rows", retained)?;
    out.insert("dropped_rows", dropped.clone())?;
    if !dropped.is_empty() {
        ext_php_rs::error::php_error(
            &ext_php_rs::flags::ErrorType::Warning,
            &format!(
                "Phit\\model_matrix dropped {} of {} observations with missing values (listwise deletion)",
                dropped.len(),
                design.n_input
            ),
        );
    }
    Ok(out)
}

/// Linear model from a formula: the fit_ols core plus coefficient names.
#[php_function]
#[php(name = "Phit\\fit_lm", defaults(missing = "\"drop\""))]
pub fn fit_lm(
    formula: &str,
    data: &Zval,
    missing: &str,
    weights: Option<&Zval>,
) -> PhpResult<ZBox<ZendHashTable>> {
    let Prepared { design, weights } = prepare(formula, data, missing, weights, true)?;
    reject_random(&design, "Phit\\fit_lm")?;
    let y = numeric_response(&design, "Phit\\fit_lm")?;
    let offset = design.offset.clone().unwrap_or_default();
    let result = stats_core::fit_ols_with_data(design.x.clone(), y, weights, offset)
        .map_err(stats_exception)?;
    let mut output = crate::ols_output(result)?;
    add_names(&mut output, &design)?;
    let (retained, dropped) = rows(&design);
    finish_result(output, design.n_input, retained, dropped, "Phit\\fit_lm")
}

fn binomial_response(design: &ModelDesign, weights: &mut [f64]) -> PhpResult<Vec<f64>> {
    let y = match &design.response {
        Some(Response::Numeric(v)) => v.clone(),
        Some(Response::Factor { codes, .. }) => {
            // R: the first level is failure and every other level success.
            codes.iter().map(|c| f64::from(u8::from(*c != 0))).collect()
        }
        Some(Response::Binomial {
            successes,
            failures,
        }) => {
            let mut y = Vec::with_capacity(successes.len());
            for ((s, f), w) in successes.iter().zip(failures).zip(weights.iter_mut()) {
                if *s < 0.0 || *f < 0.0 || s.fract() != 0.0 || f.fract() != 0.0 || s + f <= 0.0 {
                    return Err(invalid(
                        "cbind(successes, failures) needs nonnegative integer counts with positive totals",
                    ));
                }
                y.push(s / (s + f));
                *w *= s + f;
            }
            y
        }
        None => return Err(invalid("Phit\\fit_glm needs a response: 'y ~ ...'")),
    };
    if y.iter().any(|v| !(0.0..=1.0).contains(v)) {
        return Err(invalid("binary/grouped-binomial y must be in [0,1]"));
    }
    if weights.iter().any(|w| w.fract() != 0.0) {
        return Err(invalid("binomial weights must be integer trial counts"));
    }
    if y.iter()
        .zip(weights.iter())
        .any(|(v, w)| (v * w - (v * w).round()).abs() > 1e-8)
    {
        return Err(invalid(
            "binary/grouped-binomial outcomes require integer trial counts and successes",
        ));
    }
    Ok(y)
}

/// Generalized linear model from a formula over the existing GLM cores.
#[php_function]
#[php(
    name = "Phit\\fit_glm",
    defaults(family = "\"gaussian\"", link = "\"\"", missing = "\"drop\"")
)]
pub fn fit_glm(
    formula: &str,
    data: &Zval,
    family: &str,
    link: &str,
    missing: &str,
    weights: Option<&Zval>,
) -> PhpResult<ZBox<ZendHashTable>> {
    let link = match (family, link) {
        ("gaussian", "" | "identity") => "identity",
        ("binomial", "" | "logit") => "logit",
        ("binomial", "probit") => "probit",
        ("poisson", "" | "log") => "log",
        ("gaussian" | "binomial" | "poisson", other) => {
            return Err(input_exception(
                ErrorKind::Unsupported,
                &format!(
                    "link '{other}' is not supported for family '{family}'; supported: gaussian/identity, binomial/logit, binomial/probit, poisson/log"
                ),
            ));
        }
        _ => {
            return Err(input_exception(
                ErrorKind::Unsupported,
                &format!("family '{family}' is not supported; use gaussian, binomial or poisson"),
            ));
        }
    };
    let Prepared {
        design,
        mut weights,
    } = prepare(formula, data, missing, weights, true)?;
    reject_random(&design, "Phit\\fit_glm")?;
    let offset = design.offset.clone().unwrap_or_default();
    let x = design.x.clone();
    let mut output = match link {
        "identity" => {
            let y = numeric_response(&design, "Phit\\fit_glm")?;
            let n = y.len();
            let off = if offset.is_empty() {
                vec![0.0; n]
            } else {
                offset.clone()
            };
            let result = stats_core::fit_ols_with_data(x, y.clone(), weights.clone(), offset)
                .map_err(stats_exception)?;
            let p = result.coefficients.len();
            let deviance = result.rss;
            // R's glm null deviance: intercept-plus-offset fit, else the offset alone.
            let centered: Vec<f64> = y.iter().zip(&off).map(|(y, o)| y - o).collect();
            let center = if design.intercept {
                centered
                    .iter()
                    .zip(&weights)
                    .map(|(v, w)| v * w)
                    .sum::<f64>()
                    / weights.iter().sum::<f64>()
            } else {
                0.0
            };
            let null_deviance = centered
                .iter()
                .zip(&weights)
                .map(|(v, w)| w * (v - center).powi(2))
                .sum::<f64>();
            let aic = -2.0 * result.log_likelihood + 2.0 * (p as f64 + 1.0);
            let mut output = crate::ols_output(result)?;
            output.insert("deviance", deviance)?;
            output.insert("null_deviance", null_deviance)?;
            output.insert("aic", aic)?;
            output
        }
        "logit" | "probit" => {
            let y = binomial_response(&design, &mut weights)?;
            if link == "logit" {
                glm_output!(
                    stats_core::fit_logistic_with_data(x, y, weights, offset)
                        .map_err(stats_exception)?
                )
            } else {
                glm_output!(
                    stats_core::fit_probit_with_data(x, y, weights, offset)
                        .map_err(stats_exception)?
                )
            }
        }
        _ => {
            let y = numeric_response(&design, "Phit\\fit_glm")?;
            if y.iter().any(|v| *v < 0.0 || v.fract() != 0.0) {
                return Err(invalid("Poisson y must contain nonnegative integer counts"));
            }
            glm_output!(
                stats_core::fit_poisson_with_data(x, y, weights, offset)
                    .map_err(stats_exception)?
            )
        }
    };
    output.insert("family", family)?;
    output.insert("link", link)?;
    add_names(&mut output, &design)?;
    let (retained, dropped) = rows(&design);
    finish_result(output, design.n_input, retained, dropped, "Phit\\fit_glm")
}

fn group_indices(groups: &[GroupLabel]) -> (Vec<usize>, Vec<crate::GroupLabel>) {
    // First-appearance order after deletion, as fit_mixed/fit_logistic_mixed use.
    let mut labels = Vec::new();
    let mut map = std::collections::HashMap::new();
    let indices = groups
        .iter()
        .map(|g| {
            *map.entry(g.clone()).or_insert_with(|| {
                labels.push(match g {
                    GroupLabel::Integer(v) => crate::GroupLabel::Integer(*v),
                    GroupLabel::Text(v) => crate::GroupLabel::Text(v.clone()),
                });
                labels.len() - 1
            })
        })
        .collect();
    (indices, labels)
}

fn require_random(design: &ModelDesign, function: &str) -> PhpResult<()> {
    if design.random.is_empty() {
        return Err(invalid(&format!(
            "{function} needs at least one random-effect term such as (1 | g); use Phit\\fit_lm or Phit\\fit_glm for fixed effects only"
        )));
    }
    Ok(())
}

fn positive_count(value: i64, what: &str, kind: ErrorKind) -> PhpResult<usize> {
    usize::try_from(value).map_err(|_| input_exception(kind, &format!("{what} must be positive")))
}

/// Gaussian mixed model from an lme4-style formula over the fit_mixed core.
#[php_function]
#[php(
    name = "Phit\\fit_lmer",
    defaults(
        reml = true,
        missing = "\"drop\"",
        maxEvaluations = 20000,
        tolerance = 1e-7,
        optimizer = "\"bobyqa\"",
        objectiveTolerance = 1e-12
    )
)]
#[allow(clippy::too_many_arguments)] // Named PHP arguments mirror fit_mixed.
pub fn fit_lmer(
    formula: &str,
    data: &Zval,
    reml: bool,
    missing: &str,
    weights: Option<&Zval>,
    start: Option<&ZendHashTable>,
    maxEvaluations: i64,
    tolerance: f64,
    optimizer: &str,
    objectiveTolerance: f64,
) -> PhpResult<ZBox<ZendHashTable>> {
    let optimizer = match optimizer {
        "bobyqa" => stats_core::LmmOptimizer::Bobyqa,
        "nelder_mead" => stats_core::LmmOptimizer::NelderMead,
        _ => {
            return Err(input_exception(
                ErrorKind::Unsupported,
                "optimizer must be 'bobyqa' or 'nelder_mead'",
            ));
        }
    };
    let start = start
        .map(|v| {
            v.values()
                .map(|v| {
                    crate::number(v)?.ok_or_else(|| invalid("start cannot contain missing values"))
                })
                .collect::<PhpResult<Vec<_>>>()
        })
        .transpose()?;
    let Prepared { design, weights } = prepare(formula, data, missing, weights, true)?;
    require_random(&design, "Phit\\fit_lmer")?;
    let y = numeric_response(&design, "Phit\\fit_lmer")?;
    let mut term_labels = Vec::new();
    let terms = design
        .random
        .iter()
        .map(|term| {
            let (groups, labels) = group_indices(&term.groups);
            term_labels.push(labels);
            stats_core::RandomTerm {
                z: term.z.clone(),
                groups,
                correlated: term.correlated,
            }
        })
        .collect();
    let options = stats_core::LmmOptions {
        reml,
        optimizer,
        objective_tolerance: objectiveTolerance,
        offset: design.offset.clone().unwrap_or_else(|| vec![0.0; y.len()]),
        weights,
        start,
        max_evaluations: positive_count(maxEvaluations, "maxEvaluations", ErrorKind::InvalidInput)?,
        tolerance,
    };
    let result =
        stats_core::fit_lmm(design.x.clone(), y, terms, options).map_err(stats_exception)?;
    let mut output = crate::lmm_output(result, reml, term_labels)?;
    if let Some(random) = output.get_mut("random").and_then(|v| v.array_mut()) {
        for (i, term) in design.random.iter().enumerate() {
            if let Some(t) = random.get_index_mut(i as i64).and_then(|v| v.array_mut()) {
                t.insert("label", term.label.clone())?;
                t.insert("group", term.group.clone())?;
                t.insert("columns", term.columns.clone())?;
                t.insert("correlated", term.correlated)?;
            }
        }
    }
    add_names(&mut output, &design)?;
    let (retained, dropped) = rows(&design);
    finish_result(output, design.n_input, retained, dropped, "Phit\\fit_lmer")
}

/// Binomial random-intercept model from a formula over the fit_logistic_mixed core.
#[php_function]
#[php(
    name = "Phit\\fit_glmer",
    defaults(
        family = "\"binomial\"",
        link = "\"\"",
        missing = "\"drop\"",
        quadraturePoints = 1,
        maxEvaluations = 20000,
        tolerance = 1e-7
    )
)]
#[allow(clippy::too_many_arguments)] // Named PHP arguments mirror fit_logistic_mixed.
pub fn fit_glmer(
    formula: &str,
    data: &Zval,
    family: &str,
    link: &str,
    missing: &str,
    weights: Option<&Zval>,
    quadraturePoints: i64,
    maxEvaluations: i64,
    tolerance: f64,
) -> PhpResult<ZBox<ZendHashTable>> {
    if family != "binomial" || !matches!(link, "" | "logit") {
        return Err(input_exception(
            ErrorKind::Unsupported,
            &format!(
                "Phit\\fit_glmer supports family 'binomial' with the logit link only (got '{family}'/'{link}')"
            ),
        ));
    }
    let Prepared {
        design,
        mut weights,
    } = prepare(formula, data, missing, weights, true)?;
    require_random(&design, "Phit\\fit_glmer")?;
    if design.random.len() != 1 || design.random[0].columns != ["(Intercept)"] {
        return Err(input_exception(
            ErrorKind::Unsupported,
            "Phit\\fit_glmer supports exactly one random intercept, (1 | g); random slopes and several terms are not implemented",
        ));
    }
    let y = binomial_response(&design, &mut weights)?;
    let (indices, labels) = group_indices(&design.random[0].groups);
    let options = stats_core::LogisticMixedOptions {
        offset: design.offset.clone().unwrap_or_else(|| vec![0.0; y.len()]),
        weights,
        quadrature_points: positive_count(
            quadraturePoints,
            "quadraturePoints",
            ErrorKind::Unsupported,
        )?,
        max_evaluations: positive_count(maxEvaluations, "maxEvaluations", ErrorKind::InvalidInput)?,
        tolerance,
    };
    let r = stats_core::fit_logistic_random_intercept(design.x.clone(), y, indices, options)
        .map_err(stats_exception)?;
    let mut output = crate::logistic_mixed_output(r, &labels)?;
    let mut random = ZendHashTable::new();
    let mut term = ZendHashTable::new();
    term.insert("label", design.random[0].label.clone())?;
    term.insert("group", design.random[0].group.clone())?;
    term.insert("columns", design.random[0].columns.clone())?;
    random.push(term)?;
    output.insert("random", random)?;
    add_names(&mut output, &design)?;
    let (retained, dropped) = rows(&design);
    finish_result(output, design.n_input, retained, dropped, "Phit\\fit_glmer")
}

pub fn register(module: ModuleBuilder) -> ModuleBuilder {
    module
        .function(wrap_function!(model_matrix))
        .function(wrap_function!(fit_lm))
        .function(wrap_function!(fit_glm))
        .function(wrap_function!(fit_lmer))
        .function(wrap_function!(fit_glmer))
        .function(wrap_function!(predict))
}
