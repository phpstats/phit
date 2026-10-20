//! Strict PHP array conversion; numerical validation stays in stats-core.
use crate::{input_exception, stats_exception};
use ext_php_rs::{
    boxed::ZBox,
    prelude::*,
    types::{ZendHashTable, Zval},
};
use stats_core::{
    ErrorKind,
    draws::{self, Normalization},
};

fn number(value: &Zval) -> PhpResult<f64> {
    let value = value
        .double()
        .or_else(|| value.long().map(|v| v as f64))
        .ok_or_else(|| {
            input_exception(
                ErrorKind::InvalidInput,
                "draw inputs must contain integers or floats; null is not allowed",
            )
        })?;
    if !value.is_finite() {
        return Err(input_exception(
            ErrorKind::Nonfinite,
            "draw inputs must contain finite numbers",
        ));
    }
    Ok(value)
}

fn vector(input: &ZendHashTable) -> PhpResult<Vec<f64>> {
    input.values().map(number).collect()
}

fn matrix(input: &ZendHashTable) -> PhpResult<Vec<Vec<f64>>> {
    input
        .values()
        .map(|row| {
            vector(row.array().ok_or_else(|| {
                input_exception(ErrorKind::InvalidInput, "each matrix row must be an array")
            })?)
        })
        .collect()
}

#[php_function]
#[php(
    name = "Phit\\draws_moments",
    defaults(normalization = "\"population\"")
)]
pub fn draws_moments(
    draws: &ZendHashTable,
    weights: Option<&ZendHashTable>,
    normalization: &str,
) -> PhpResult<ZBox<ZendHashTable>> {
    let normalization = match normalization {
        "population" => Normalization::Population,
        "unbiased" => Normalization::Unbiased,
        _ => {
            return Err(input_exception(
                ErrorKind::InvalidInput,
                "normalization must be 'population' or 'unbiased'",
            ));
        }
    };
    let draws = matrix(draws)?;
    let weights = weights.map(vector).transpose()?;
    let result =
        draws::draw_moments(&draws, weights.as_deref(), normalization).map_err(stats_exception)?;
    let mut output = ZendHashTable::new();
    output.insert("mean", result.mean)?;
    output.insert("covariance", result.covariance)?;
    Ok(output)
}

#[php_function]
#[php(name = "Phit\\draws_covariance_map")]
pub fn draws_covariance_map(
    sourceCovariance: &ZendHashTable,
    targetCovariance: &ZendHashTable,
) -> PhpResult<ZBox<ZendHashTable>> {
    let result = draws::draw_covariance_map(&matrix(sourceCovariance)?, &matrix(targetCovariance)?)
        .map_err(stats_exception)?;
    let mut output = ZendHashTable::new();
    output.insert("matrix", result.matrix)?;
    output.insert("log_abs_det", result.log_abs_det)?;
    Ok(output)
}

#[php_function]
#[php(name = "Phit\\draws_affine")]
pub fn draws_affine(
    draws: &ZendHashTable,
    matrix: &ZendHashTable,
    shift: &ZendHashTable,
) -> PhpResult<ZBox<ZendHashTable>> {
    let result = draws::draw_affine(
        &self::matrix(draws)?,
        &self::matrix(matrix)?,
        &vector(shift)?,
    )
    .map_err(stats_exception)?;
    let mut output = ZendHashTable::new();
    output.insert("draws", result.draws)?;
    output.insert("log_abs_det", result.log_abs_det)?;
    Ok(output)
}

pub fn register(module: ModuleBuilder) -> ModuleBuilder {
    module
        .function(wrap_function!(draws_moments))
        .function(wrap_function!(draws_covariance_map))
        .function(wrap_function!(draws_affine))
}
