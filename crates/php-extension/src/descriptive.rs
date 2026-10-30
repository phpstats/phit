//! PHP bindings for descriptive statistics.
//!
//! null and NAN are missing values. missing: 'raise' (the default) rejects them;
//! 'drop' removes them, pairwise for two-array functions. These functions return
//! a single number without a sample count, so dropping is opt-in rather than
//! the estimators' warn-and-drop default. Infinite values are always errors.
use crate::distributions::Real;
use crate::stats_exception;
use ext_php_rs::{boxed::ZBox, prelude::*, types::ZendHashTable, types::Zval};
use stats_core::descriptive::{self as desc, WeightKind};
use stats_core::{ErrorKind, StatsError};

fn error(kind: ErrorKind, message: &str) -> PhpException {
    stats_exception(StatsError::new(kind, message))
}

pub(crate) fn element(value: &Zval) -> PhpResult<Option<f64>> {
    if value.is_null() {
        return Ok(None);
    }
    let value = value
        .double()
        .or_else(|| value.long().map(|v| v as f64))
        .ok_or_else(|| {
            error(
                ErrorKind::InvalidInput,
                "values must be integers, floats, or null",
            )
        })?;
    if value.is_nan() {
        Ok(None)
    } else if value.is_infinite() {
        Err(error(
            ErrorKind::Nonfinite,
            "infinite values are not allowed",
        ))
    } else {
        Ok(Some(value))
    }
}

pub(crate) fn drop_missing(missing: &str) -> PhpResult<bool> {
    match missing {
        "drop" => Ok(true),
        "raise" => Ok(false),
        _ => Err(error(
            ErrorKind::InvalidInput,
            "missing must be 'raise' or 'drop'",
        )),
    }
}

pub(crate) fn missing_error() -> PhpException {
    error(
        ErrorKind::InvalidInput,
        "missing values found with missing='raise'",
    )
}

pub(crate) fn values(input: &ZendHashTable, missing: &str) -> PhpResult<Vec<f64>> {
    let drop = drop_missing(missing)?;
    let mut out = Vec::with_capacity(input.len());
    for value in input.values() {
        match element(value)? {
            Some(v) => out.push(v),
            None if drop => {}
            None => return Err(missing_error()),
        }
    }
    Ok(out)
}

// Two aligned arrays with pairwise deletion of missing values.
pub(crate) fn pairs(
    x: &ZendHashTable,
    y: &ZendHashTable,
    missing: &str,
) -> PhpResult<(Vec<f64>, Vec<f64>)> {
    let drop = drop_missing(missing)?;
    if x.len() != y.len() {
        return Err(error(
            ErrorKind::DimensionMismatch,
            "arrays must have the same length",
        ));
    }
    let (mut a, mut b) = (Vec::with_capacity(x.len()), Vec::with_capacity(y.len()));
    for (u, v) in x.values().zip(y.values()) {
        match (element(u)?, element(v)?) {
            (Some(u), Some(v)) => {
                a.push(u);
                b.push(v);
            }
            _ if drop => {}
            _ => return Err(missing_error()),
        }
    }
    Ok((a, b))
}

fn weighted_pairs(
    values: &ZendHashTable,
    weights: &ZendHashTable,
    missing: &str,
) -> PhpResult<(Vec<f64>, Vec<f64>)> {
    // Invalid weights cannot disappear just because the aligned value is missing.
    for value in weights.values() {
        if element(value)?.is_some_and(|w| w < 0.0) {
            return Err(error(
                ErrorKind::InvalidInput,
                "weights must be nonnegative",
            ));
        }
    }
    pairs(values, weights, missing)
}

fn population(kind: &str) -> PhpResult<bool> {
    match kind {
        "sample" => Ok(false),
        "population" => Ok(true),
        _ => Err(error(
            ErrorKind::InvalidInput,
            "type must be 'sample' or 'population'",
        )),
    }
}

fn result(value: Result<f64, StatsError>) -> PhpResult<f64> {
    value.map_err(stats_exception)
}

#[php_function]
#[php(name = "Phit\\stat_mean", defaults(missing = "\"raise\""))]
pub fn stat_mean(values: &ZendHashTable, missing: &str) -> PhpResult<f64> {
    result(desc::mean(&self::values(values, missing)?))
}

#[php_function]
#[php(
    name = "Phit\\stat_var",
    defaults(r#type = "\"sample\"", missing = "\"raise\"")
)]
pub fn stat_var(values: &ZendHashTable, r#type: &str, missing: &str) -> PhpResult<f64> {
    result(desc::variance(
        &self::values(values, missing)?,
        population(r#type)?,
    ))
}

#[php_function]
#[php(
    name = "Phit\\stat_sd",
    defaults(r#type = "\"sample\"", missing = "\"raise\"")
)]
pub fn stat_sd(values: &ZendHashTable, r#type: &str, missing: &str) -> PhpResult<f64> {
    result(desc::sd(
        &self::values(values, missing)?,
        population(r#type)?,
    ))
}

#[php_function]
#[php(
    name = "Phit\\stat_cov",
    defaults(r#type = "\"sample\"", missing = "\"raise\"")
)]
pub fn stat_cov(
    x: &ZendHashTable,
    y: &ZendHashTable,
    r#type: &str,
    missing: &str,
) -> PhpResult<f64> {
    let (x, y) = pairs(x, y, missing)?;
    result(desc::covariance(&x, &y, population(r#type)?))
}

#[php_function]
#[php(name = "Phit\\stat_cor", defaults(missing = "\"raise\""))]
pub fn stat_cor(x: &ZendHashTable, y: &ZendHashTable, missing: &str) -> PhpResult<f64> {
    let (x, y) = pairs(x, y, missing)?;
    result(desc::correlation(&x, &y))
}

#[php_function]
#[php(name = "Phit\\stat_quantile", defaults(missing = "\"raise\""))]
pub fn stat_quantile(values: &ZendHashTable, p: Real, missing: &str) -> PhpResult<f64> {
    result(desc::quantile(&self::values(values, missing)?, p.0))
}

#[php_function]
#[php(name = "Phit\\stat_quantile_array", defaults(missing = "\"raise\""))]
pub fn stat_quantile_array(
    values: &ZendHashTable,
    probabilities: &ZendHashTable,
    missing: &str,
) -> PhpResult<ZBox<ZendHashTable>> {
    let probabilities = probabilities
        .values()
        .map(|v| {
            v.double()
                .or_else(|| v.long().map(|v| v as f64))
                .ok_or_else(|| error(ErrorKind::InvalidInput, "probabilities must be numbers"))
        })
        .collect::<PhpResult<Vec<_>>>()?;
    let quantiles = desc::quantiles(&self::values(values, missing)?, &probabilities)
        .map_err(stats_exception)?;
    let mut out = ZendHashTable::with_capacity(quantiles.len() as u32);
    for q in quantiles {
        out.push(q)?;
    }
    Ok(out)
}

#[php_function]
#[php(name = "Phit\\stat_median", defaults(missing = "\"raise\""))]
pub fn stat_median(values: &ZendHashTable, missing: &str) -> PhpResult<f64> {
    result(desc::median(&self::values(values, missing)?))
}

#[php_function]
#[php(name = "Phit\\stat_weighted_mean", defaults(missing = "\"raise\""))]
pub fn stat_weighted_mean(
    values: &ZendHashTable,
    weights: &ZendHashTable,
    missing: &str,
) -> PhpResult<f64> {
    let (values, weights) = weighted_pairs(values, weights, missing)?;
    result(desc::weighted_mean(&values, &weights))
}

#[php_function]
#[php(
    name = "Phit\\stat_weighted_var",
    defaults(r#type = "\"frequency\"", missing = "\"raise\"")
)]
pub fn stat_weighted_var(
    values: &ZendHashTable,
    weights: &ZendHashTable,
    r#type: &str,
    missing: &str,
) -> PhpResult<f64> {
    let kind = match r#type {
        "frequency" => WeightKind::Frequency,
        "reliability" => WeightKind::Reliability,
        "population" => WeightKind::Population,
        _ => {
            return Err(error(
                ErrorKind::InvalidInput,
                "type must be 'frequency', 'reliability' or 'population'",
            ));
        }
    };
    let (values, weights) = weighted_pairs(values, weights, missing)?;
    result(desc::weighted_variance(&values, &weights, kind))
}

pub fn register(module: ModuleBuilder) -> ModuleBuilder {
    module
        .function(wrap_function!(stat_mean))
        .function(wrap_function!(stat_var))
        .function(wrap_function!(stat_sd))
        .function(wrap_function!(stat_cov))
        .function(wrap_function!(stat_cor))
        .function(wrap_function!(stat_quantile))
        .function(wrap_function!(stat_quantile_array))
        .function(wrap_function!(stat_median))
        .function(wrap_function!(stat_weighted_mean))
        .function(wrap_function!(stat_weighted_var))
}
