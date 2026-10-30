// PHP parameter names are the public API and the conventions make them camelCase
// (maxEvaluations, seFit). ext-php-rs derives each PHP name from the Rust parameter
// identifier and rejects #[php(...)] on a parameter, so the identifiers must be
// camelCase too. #[php_function] repeats them in a generated wrapper beside each
// function, which no per-function attribute reaches, so the allow is crate-level.
#![allow(non_snake_case)]

use ext_php_rs::{boxed::ZBox, prelude::*, types::ZendHashTable};
mod csv;
mod descriptive;
mod distributions;
mod draws;
mod smoothing;
fn stats_exception(e: stats_core::StatsError) -> PhpException {
    PhpException::new(
        e.to_string(),
        e.kind() as i32,
        ext_php_rs::zend::ce::exception(),
    )
}
fn input_exception(kind: stats_core::ErrorKind, message: &str) -> PhpException {
    PhpException::new(
        message.into(),
        kind as i32,
        ext_php_rs::zend::ce::exception(),
    )
}

// null and NaN are missing; infinity and nonnumeric values remain errors.
fn number(value: &ext_php_rs::types::Zval) -> PhpResult<Option<f64>> {
    if value.is_null() {
        return Ok(None);
    }
    let value = value
        .double()
        .or_else(|| value.long().map(|v| v as f64))
        .ok_or_else(|| {
            input_exception(
                stats_core::ErrorKind::InvalidInput,
                "inputs must contain integers, floats, or null",
            )
        })?;
    if value.is_nan() {
        Ok(None)
    } else if !value.is_finite() {
        Err(input_exception(
            stats_core::ErrorKind::Nonfinite,
            "infinite input values are not allowed",
        ))
    } else {
        Ok(Some(value))
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Hash)]
enum GroupLabel {
    Integer(i64),
    Text(String),
}

fn labels(values: &ZendHashTable) -> PhpResult<Vec<Option<GroupLabel>>> {
    values
        .iter()
        .map(|(_, v)| {
            if v.is_null() {
                Ok(None)
            } else if let Some(id) = v.long() {
                Ok(Some(GroupLabel::Integer(id)))
            } else if let Some(id) = v.str() {
                Ok(Some(GroupLabel::Text(id.to_owned())))
            } else {
                Err(input_exception(
                    stats_core::ErrorKind::InvalidInput,
                    "group IDs must be integers, strings, or null",
                ))
            }
        })
        .collect()
}
fn numeric_rows(values: &ZendHashTable) -> PhpResult<Vec<Vec<Option<f64>>>> {
    values
        .iter()
        .map(|(_, row)| {
            row.array()
                .ok_or_else(|| {
                    input_exception(
                        stats_core::ErrorKind::InvalidInput,
                        "each matrix row must be an array",
                    )
                })?
                .iter()
                .map(|(_, v)| number(v))
                .collect()
        })
        .collect()
}
fn numeric_vector(
    values: Option<&ZendHashTable>,
    n: usize,
    default: f64,
) -> PhpResult<Vec<Option<f64>>> {
    let result = match values {
        Some(v) => v
            .iter()
            .map(|(_, v)| number(v))
            .collect::<PhpResult<Vec<_>>>()?,
        None => vec![Some(default); n],
    };
    if result.len() != n {
        return Err(input_exception(
            stats_core::ErrorKind::DimensionMismatch,
            "optional vectors must have one value per original observation",
        ));
    }
    Ok(result)
}
fn label_values(values: &[GroupLabel]) -> PhpResult<ZBox<ZendHashTable>> {
    let mut out = ZendHashTable::new();
    for v in values {
        match v {
            GroupLabel::Integer(v) => out.push(*v)?,
            GroupLabel::Text(v) => out.push(v.clone())?,
        };
    }
    Ok(out)
}
struct PreparedData {
    x: Vec<Vec<f64>>,
    y: Vec<f64>,
    groups: Vec<GroupLabel>,
    weights: Vec<f64>,
    offset: Vec<f64>,
    n_input: usize,
    retained_rows: Vec<i64>,
    dropped_rows: Vec<i64>,
}

#[allow(clippy::too_many_arguments)] // Literal PHP options share one validation/deletion path.
fn prepare_data(
    x: &ZendHashTable,
    y: &ZendHashTable,
    missing: &str,
    outcome: &str,
    groups: Option<&ZendHashTable>,
    weights: Option<&ZendHashTable>,
    offset: Option<&ZendHashTable>,
    intercept: bool,
) -> PhpResult<PreparedData> {
    if missing != "drop" && missing != "raise" {
        return Err(input_exception(
            stats_core::ErrorKind::InvalidInput,
            "missing must be 'drop' or 'raise'",
        ));
    }
    // Validate every value and the original shape before removing any rows.
    let rows = x
        .iter()
        .map(|(_, row)| {
            row.array()
                .ok_or_else(|| {
                    input_exception(
                        stats_core::ErrorKind::InvalidInput,
                        "each x row must be an array",
                    )
                })?
                .iter()
                .map(|(_, value)| number(value))
                .collect::<PhpResult<Vec<_>>>()
        })
        .collect::<PhpResult<Vec<_>>>()?;
    let outcomes = y
        .iter()
        .map(|(_, value)| {
            let value = number(value)?;
            if outcome == "binary" && value.is_some_and(|v| !(0.0..=1.0).contains(&v)) {
                return Err(input_exception(
                    stats_core::ErrorKind::InvalidInput,
                    "binary/grouped-binomial y must be in [0,1]",
                ));
            }
            if outcome == "count" && value.is_some_and(|v| v < 0.0 || v.fract() != 0.0) {
                return Err(input_exception(
                    stats_core::ErrorKind::InvalidInput,
                    "Poisson y must contain nonnegative integer counts",
                ));
            }
            Ok(value)
        })
        .collect::<PhpResult<Vec<_>>>()?;
    let labels = groups.map(labels).transpose()?;
    let n = rows.len();
    if n == 0 || (!intercept && rows[0].is_empty()) {
        return Err(input_exception(
            stats_core::ErrorKind::InvalidInput,
            "x must have at least one row; without an intercept, at least one column is required",
        ));
    }
    if labels.as_ref().is_some_and(|labels| labels.len() != n) {
        return Err(input_exception(
            stats_core::ErrorKind::DimensionMismatch,
            "groups must have one identifier per observation",
        ));
    }
    if outcomes.len() != n {
        return Err(input_exception(
            stats_core::ErrorKind::DimensionMismatch,
            "x and y must have the same number of observations",
        ));
    }
    if rows.iter().any(|row| row.len() != rows[0].len()) {
        return Err(input_exception(
            stats_core::ErrorKind::DimensionMismatch,
            "x must be rectangular",
        ));
    }
    let weights = numeric_vector(weights, n, 1.0)?;
    let offset = numeric_vector(offset, n, 0.0)?;
    if weights
        .iter()
        .flatten()
        .any(|v| *v <= 0.0 || (outcome == "binary" && v.fract() != 0.0))
    {
        return Err(input_exception(
            stats_core::ErrorKind::InvalidInput,
            "weights must be positive; binary weights must be integer trial counts",
        ));
    }
    if outcome == "binary"
        && weights
            .iter()
            .flatten()
            .any(|w| *w > stats_core::MAX_LOGISTIC_MIXED_TRIALS)
    {
        return Err(input_exception(
            stats_core::ErrorKind::Unsupported,
            "binomial trial weights must not exceed 1000000",
        ));
    }
    if outcome == "binary"
        && outcomes.iter().zip(&weights).any(|(v, w)| {
            v.zip(*w)
                .is_some_and(|(v, w)| (v * w - (v * w).round()).abs() > 1e-8)
        })
    {
        return Err(input_exception(
            stats_core::ErrorKind::InvalidInput,
            "binary/grouped-binomial outcomes require integer trial counts and successes",
        ));
    }
    let mut complete_weights = Vec::new();
    let mut complete_offset = Vec::new();
    let mut complete_groups = Vec::new();
    let mut complete_x = Vec::new();
    let mut complete_y = Vec::new();
    let mut retained_rows = Vec::new();
    let mut dropped_rows = Vec::new();
    for (i, (row, outcome)) in rows.into_iter().zip(outcomes).enumerate() {
        if outcome.is_none()
            || row.iter().any(Option::is_none)
            || labels.as_ref().is_some_and(|v| v[i].is_none())
            || weights[i].is_none()
            || offset[i].is_none()
        {
            dropped_rows.push(i as i64);
        } else {
            // Collect only complete rows; no missing sentinel reaches stats-core.
            // Reserve the intercept column up front instead of shifting each row later.
            let mut design_row = Vec::with_capacity(row.len() + usize::from(intercept));
            if intercept {
                design_row.push(1.0);
            }
            design_row.extend(row.into_iter().flatten());
            complete_x.push(design_row);
            complete_y.extend(outcome);
            complete_weights.extend(weights[i]);
            complete_offset.extend(offset[i]);
            if let Some(labels) = &labels
                && let Some(label) = &labels[i]
            {
                complete_groups.push(label.clone());
            }
            retained_rows.push(i as i64);
        }
    }
    if missing == "raise" && !dropped_rows.is_empty() {
        return Err(input_exception(
            stats_core::ErrorKind::InvalidInput,
            "missing values found with missing='raise'",
        ));
    }
    Ok(PreparedData {
        groups: complete_groups,
        weights: complete_weights,
        offset: complete_offset,
        x: complete_x,
        y: complete_y,
        n_input: n,
        retained_rows,
        dropped_rows,
    })
}

// The PHP convenience option constructs the fixed design only. stats-core and
// random-effect designs continue to receive literal mathematical matrices.
fn fixed_design(mut rows: Vec<Vec<f64>>, intercept: bool) -> Vec<Vec<f64>> {
    if intercept {
        for row in &mut rows {
            row.insert(0, 1.0);
        }
    }
    rows
}

fn finish_result(
    mut output: ZBox<ZendHashTable>,
    n: usize,
    retained_rows: Vec<i64>,
    dropped_rows: Vec<i64>,
    function: &str,
) -> PhpResult<ZBox<ZendHashTable>> {
    // Additive diagnostics: keep all historical top-level model-specific keys.
    let mut diagnostics = ZendHashTable::new();
    diagnostics.insert("converged", true)?;
    diagnostics.insert(
        "solver",
        match function {
            "Phit\\fit_ols" | "Phit\\fit_lm" => "scaled_svd",
            "Phit\\fit_glm" => match output.get("link").and_then(|v| v.str()) {
                Some("identity") => "scaled_svd",
                Some("probit") => "fisher_scoring",
                _ => "newton_irls",
            },
            "Phit\\fit_logistic" | "Phit\\fit_poisson" => "newton_irls",
            "Phit\\fit_probit" => "fisher_scoring",
            "Phit\\fit_mixed_intercept" => "profile_variance_search",
            _ => output
                .get("optimizer")
                .and_then(|v| v.str())
                .unwrap_or("unknown"),
        },
    )?;
    for key in [
        "iterations",
        "evaluations",
        "gradient_max",
        "boundary",
        "singular",
    ] {
        if let Some(value) = output.get(key) {
            diagnostics.insert(key, value.shallow_clone())?;
        }
    }
    let mut warnings = Vec::<String>::new();
    if !dropped_rows.is_empty() {
        warnings.push(format!(
            "listwise deletion removed {} of {} original rows",
            dropped_rows.len(),
            n
        ));
    }
    if output.get("singular").and_then(|v| v.bool()) == Some(true) {
        warnings.push("random-effect covariance is singular or near singular".into());
    }
    if matches!(function, "Phit\\fit_logistic_mixed" | "Phit\\fit_glmer")
        && output.get("quadrature_points").and_then(|v| v.long()) == Some(1)
    {
        warnings.push("Laplace integration is approximate; assess quadrature sensitivity for rare events or small groups".into());
    }
    diagnostics.insert("warnings", warnings)?;
    output.insert("diagnostics", diagnostics)?;
    output.insert("n_input", n)?;
    output.insert("n_observations", retained_rows.len())?;
    output.insert("n_dropped", dropped_rows.len())?;
    output.insert("retained_rows", retained_rows)?;
    output.insert("dropped_rows", dropped_rows.clone())?;
    if !dropped_rows.is_empty() {
        ext_php_rs::error::php_error(
            &ext_php_rs::flags::ErrorType::Warning,
            &format!(
                "{function} dropped {} of {} observations with missing values (listwise deletion)",
                dropped_rows.len(),
                n
            ),
        );
    }
    Ok(output)
}

fn ols_output(result: stats_core::OlsResult) -> PhpResult<ZBox<ZendHashTable>> {
    let mut output = ZendHashTable::new();
    macro_rules! put {
        ($key:literal, $value:expr) => {
            output
                .insert($key, $value)
                .map_err(|e| PhpException::from(e.to_string()))?;
        };
    }
    put!("coefficients", result.coefficients);
    put!("covariance", result.covariance);
    put!("std_errors", result.std_errors);
    put!("t_statistics", result.t_statistics);
    put!("p_values", result.p_values);
    put!(
        "conf_int",
        result
            .conf_int
            .into_iter()
            .map(|v| v.to_vec())
            .collect::<Vec<_>>()
    );
    put!("fitted", result.fitted);
    put!("residuals", result.residuals);
    put!("rss", result.rss);
    put!("residual_variance", result.residual_variance);
    put!("sigma", result.sigma);
    put!("r_squared", result.r_squared);
    put!("adj_r_squared", result.adj_r_squared);
    put!("df_residual", result.df_residual as i64);
    put!("log_likelihood", result.log_likelihood);
    Ok(output)
}

// The three GLM cores share result field names.
macro_rules! glm_output {
    ($result:expr) => {{
        let result = $result;
        let mut output = ZendHashTable::new();
        macro_rules! put {
            ($key:literal, $value:expr) => {
                output
                    .insert($key, $value)
                    .map_err(|e| PhpException::from(e.to_string()))?;
            };
        }
        put!("coefficients", result.coefficients);
        put!("covariance", result.covariance);
        put!("std_errors", result.std_errors);
        put!("z_statistics", result.z_statistics);
        put!("p_values", result.p_values);
        put!(
            "conf_int",
            result
                .conf_int
                .into_iter()
                .map(|v| v.to_vec())
                .collect::<Vec<_>>()
        );
        put!("fitted", result.fitted);
        put!("residuals", result.residuals);
        put!("log_likelihood", result.log_likelihood);
        put!("deviance", result.deviance);
        put!("null_deviance", result.null_deviance);
        put!("aic", result.aic);
        put!("df_residual", result.df_residual as i64);
        put!("iterations", result.iterations as i64);
        put!("converged", true);
        output
    }};
}

mod formula;

#[php_function]
#[php(
    name = "Phit\\fit_ols",
    defaults(missing = "\"drop\"", intercept = true)
)]
pub fn fit_ols(
    x: &ZendHashTable,
    y: &ZendHashTable,
    missing: &str,
    weights: Option<&ZendHashTable>,
    offset: Option<&ZendHashTable>,
    intercept: bool,
) -> PhpResult<ZBox<ZendHashTable>> {
    let input = prepare_data(
        x,
        y,
        missing,
        "continuous",
        None,
        weights,
        offset,
        intercept,
    )?;
    let result = stats_core::fit_ols_with_data(input.x, input.y, input.weights, input.offset)
        .map_err(stats_exception)?;
    let output = ols_output(result)?;
    finish_result(
        output,
        input.n_input,
        input.retained_rows,
        input.dropped_rows,
        "Phit\\fit_ols",
    )
}

#[php_function]
#[php(
    name = "Phit\\fit_logistic",
    defaults(missing = "\"drop\"", intercept = true)
)]
pub fn fit_logistic(
    x: &ZendHashTable,
    y: &ZendHashTable,
    missing: &str,
    weights: Option<&ZendHashTable>,
    offset: Option<&ZendHashTable>,
    intercept: bool,
) -> PhpResult<ZBox<ZendHashTable>> {
    let input = prepare_data(x, y, missing, "binary", None, weights, offset, intercept)?;
    let result = stats_core::fit_logistic_with_data(input.x, input.y, input.weights, input.offset)
        .map_err(stats_exception)?;
    let output = glm_output!(result);
    finish_result(
        output,
        input.n_input,
        input.retained_rows,
        input.dropped_rows,
        "Phit\\fit_logistic",
    )
}

#[php_function]
#[php(
    name = "Phit\\fit_probit",
    defaults(missing = "\"drop\"", intercept = true)
)]
pub fn fit_probit(
    x: &ZendHashTable,
    y: &ZendHashTable,
    missing: &str,
    weights: Option<&ZendHashTable>,
    offset: Option<&ZendHashTable>,
    intercept: bool,
) -> PhpResult<ZBox<ZendHashTable>> {
    let input = prepare_data(x, y, missing, "binary", None, weights, offset, intercept)?;
    let result = stats_core::fit_probit_with_data(input.x, input.y, input.weights, input.offset)
        .map_err(stats_exception)?;
    let output = glm_output!(result);
    finish_result(
        output,
        input.n_input,
        input.retained_rows,
        input.dropped_rows,
        "Phit\\fit_probit",
    )
}

#[php_function]
#[php(
    name = "Phit\\fit_poisson",
    defaults(missing = "\"drop\"", intercept = true)
)]
pub fn fit_poisson(
    x: &ZendHashTable,
    y: &ZendHashTable,
    missing: &str,
    weights: Option<&ZendHashTable>,
    offset: Option<&ZendHashTable>,
    intercept: bool,
) -> PhpResult<ZBox<ZendHashTable>> {
    let input = prepare_data(x, y, missing, "count", None, weights, offset, intercept)?;
    let result = stats_core::fit_poisson_with_data(input.x, input.y, input.weights, input.offset)
        .map_err(stats_exception)?;
    let output = glm_output!(result);
    finish_result(
        output,
        input.n_input,
        input.retained_rows,
        input.dropped_rows,
        "Phit\\fit_poisson",
    )
}

#[php_function]
#[php(
    name = "Phit\\fit_mixed_intercept",
    defaults(missing = "\"drop\"", intercept = true)
)]
pub fn fit_mixed_intercept(
    x: &ZendHashTable,
    y: &ZendHashTable,
    groups: &ZendHashTable,
    missing: &str,
    intercept: bool,
) -> PhpResult<ZBox<ZendHashTable>> {
    let input = prepare_data(
        x,
        y,
        missing,
        "continuous",
        Some(groups),
        None,
        None,
        intercept,
    )?;
    let mut group_labels = Vec::new();
    let mut label_indices = std::collections::HashMap::new();
    let indices = input
        .groups
        .iter()
        .map(|label| {
            *label_indices.entry(label.clone()).or_insert_with(|| {
                group_labels.push(label.clone());
                group_labels.len() - 1
            })
        })
        .collect();
    let result =
        stats_core::fit_random_intercept(input.x, input.y, indices).map_err(stats_exception)?;
    let mut output = ZendHashTable::new();
    macro_rules! put {
        ($key:literal, $value:expr) => {
            output
                .insert($key, $value)
                .map_err(|e| PhpException::from(e.to_string()))?;
        };
    }
    put!("coefficients", result.coefficients);
    put!("covariance", result.covariance);
    put!("std_errors", result.std_errors);
    put!("z_statistics", result.z_statistics);
    put!("p_values", result.p_values);
    put!(
        "conf_int",
        result
            .conf_int
            .into_iter()
            .map(|v| v.to_vec())
            .collect::<Vec<_>>()
    );
    put!("fitted", result.fitted);
    put!("residuals", result.residuals);
    put!("log_likelihood", result.log_likelihood);

    put!("aic", result.aic);
    put!("df_residual", result.df_residual as i64);
    put!("evaluations", result.evaluations as i64);
    put!("fixed_fitted", result.fixed_fitted);
    put!("marginal_residuals", result.marginal_residuals);
    put!("random_effects", result.random_effects);
    put!("random_effects_cond_var", result.random_effects_cond_var);
    put!("group_sizes", result.group_sizes);
    put!("n_groups", group_labels.len());
    let mut labels = ZendHashTable::new();
    for label in group_labels {
        match label {
            GroupLabel::Integer(v) => labels.push(v)?,
            GroupLabel::Text(v) => labels.push(v)?,
        }
    }
    put!("group_labels", labels);
    put!("residual_variance", result.residual_variance);
    put!(
        "random_intercept_variance",
        result.random_intercept_variance
    );
    put!("sigma", result.sigma);
    put!("icc", result.icc);
    put!("boundary", result.boundary);
    put!("method", "ML");
    put!("converged", true);
    finish_result(
        output,
        input.n_input,
        input.retained_rows,
        input.dropped_rows,
        "Phit\\fit_mixed_intercept",
    )
}

fn lmm_output(
    result: stats_core::LmmResult,
    reml: bool,
    term_labels: Vec<Vec<GroupLabel>>,
) -> PhpResult<ZBox<ZendHashTable>> {
    let mut out = ZendHashTable::new();
    out.insert("coefficients", result.coefficients)?;
    out.insert("covariance", result.covariance)?;
    out.insert("std_errors", result.std_errors)?;
    out.insert("z_statistics", result.z_statistics)?;
    out.insert("p_values", result.p_values)?;
    out.insert(
        "conf_int",
        result
            .conf_int
            .into_iter()
            .map(|v| v.to_vec())
            .collect::<Vec<_>>(),
    )?;
    out.insert("fixed_fitted", result.fixed_fitted)?;
    out.insert("fitted", result.fitted)?;
    out.insert("residuals", result.residuals)?;
    out.insert("marginal_residuals", result.marginal_residuals)?;
    out.insert("residual_variance", result.residual_variance)?;
    out.insert("sigma", result.sigma)?;
    out.insert("log_likelihood", result.log_likelihood)?;
    out.insert("aic", result.aic)?;
    out.insert("bic", result.bic)?;
    out.insert("df_residual", result.df_residual)?;
    out.insert("n_parameters", result.n_parameters)?;
    out.insert("theta", result.theta)?;
    out.insert("singular", result.singular)?;
    out.insert("method", if reml { "REML" } else { "ML" })?;
    out.insert("optimizer", result.optimizer.name())?;
    out.insert(
        "optimizer_implementation",
        match result.optimizer {
            stats_core::LmmOptimizer::Bobyqa => "bobyqa 0.3.2 / PRIMA Rust port",
            stats_core::LmmOptimizer::NelderMead => "argmin 0.11.0 / Nelder-Mead",
        },
    )?;
    out.insert("converged", true)?;
    out.insert("evaluations", result.evaluations)?;
    let mut output_terms = ZendHashTable::new();
    for (term, labels) in result.terms.into_iter().zip(term_labels) {
        let mut t = ZendHashTable::new();
        t.insert("covariance", term.covariance)?;
        t.insert("effects", term.effects)?;
        t.insert("effects_cond_var", term.effects_cond_var)?;
        t.insert("group_labels", label_values(&labels)?)?;
        t.insert("group_sizes", term.group_sizes)?;
        output_terms.push(t)?;
    }
    out.insert("random", output_terms)?;
    Ok(out)
}

fn logistic_mixed_output(
    r: stats_core::LogisticMixedResult,
    group_labels: &[GroupLabel],
) -> PhpResult<ZBox<ZendHashTable>> {
    let mut out = ZendHashTable::new();
    out.insert("coefficients", r.coefficients)?;
    out.insert("covariance", r.covariance)?;
    out.insert("std_errors", r.std_errors)?;
    out.insert("z_statistics", r.z_statistics)?;
    out.insert("p_values", r.p_values)?;
    out.insert(
        "conf_int",
        r.conf_int
            .into_iter()
            .map(|v| v.to_vec())
            .collect::<Vec<_>>(),
    )?;
    out.insert("fixed_fitted", r.fixed_fitted)?;
    out.insert("fitted", r.fitted)?;
    out.insert("linear_predictor", r.linear_predictor)?;
    out.insert("residuals", r.residuals)?;
    out.insert("random_effects", r.random_effects)?;
    out.insert("random_effects_cond_var", r.random_effects_cond_var)?;
    out.insert("group_sizes", r.group_sizes)?;
    out.insert("group_labels", label_values(group_labels)?)?;
    out.insert("n_groups", group_labels.len())?;
    out.insert("random_intercept_variance", r.random_intercept_variance)?;
    out.insert("random_intercept_sd", r.random_intercept_sd)?;
    out.insert("log_likelihood", r.log_likelihood)?;
    out.insert("neg2_log_likelihood", r.neg2_log_likelihood)?;
    out.insert("aic", r.aic)?;
    out.insert("bic", r.bic)?;
    out.insert("n_parameters", r.n_parameters)?;
    out.insert("df_residual", r.df_residual)?;
    out.insert("quadrature_points", r.quadrature_points)?;
    out.insert(
        "integration",
        if r.quadrature_points == 1 {
            "laplace"
        } else {
            "adaptive_gauss_hermite"
        },
    )?;
    out.insert("evaluations", r.evaluations)?;
    out.insert("gradient_max", r.gradient_max)?;
    out.insert("boundary", r.boundary)?;
    out.insert("singular", r.singular)?;
    out.insert("family", "binomial")?;
    out.insert("link", "logit")?;
    out.insert("method", "ML")?;
    out.insert("optimizer", "bobyqa")?;
    out.insert("converged", true)?;
    Ok(out)
}

#[php_function]
#[php(
    name = "Phit\\fit_mixed",
    defaults(
        missing = "\"drop\"",
        reml = false,
        maxEvaluations = 20000,
        tolerance = 1e-7,
        optimizer = "\"bobyqa\"",
        objectiveTolerance = 1e-12,
        intercept = true
    )
)]
// Preserve the procedural PHP signature and named arguments.
#[allow(clippy::too_many_arguments)]
pub fn fit_mixed(
    x: &ZendHashTable,
    y: &ZendHashTable,
    random: &ZendHashTable,
    missing: &str,
    reml: bool,
    weights: Option<&ZendHashTable>,
    offset: Option<&ZendHashTable>,
    start: Option<&ZendHashTable>,
    maxEvaluations: i64,
    tolerance: f64,
    optimizer: &str,
    objectiveTolerance: f64,
    intercept: bool,
) -> PhpResult<ZBox<ZendHashTable>> {
    if missing != "drop" && missing != "raise" {
        return Err(input_exception(
            stats_core::ErrorKind::InvalidInput,
            "missing must be 'drop' or 'raise'",
        ));
    }
    let x = numeric_rows(x)?;
    let n = x.len();
    if n == 0 || (!intercept && x[0].is_empty()) || x.iter().any(|r| r.len() != x[0].len()) {
        return Err(input_exception(
            stats_core::ErrorKind::DimensionMismatch,
            "x must be nonempty and rectangular",
        ));
    }
    let y = numeric_vector(Some(y), n, 0.0)?;
    let weights = numeric_vector(weights, n, 1.0)?;
    let offset = numeric_vector(offset, n, 0.0)?;
    if weights.iter().flatten().any(|v| *v <= 0.0) {
        return Err(input_exception(
            stats_core::ErrorKind::InvalidInput,
            "weights must be positive",
        ));
    }
    let mut terms = Vec::new();
    for (_, value) in random.iter() {
        let term = value.array().ok_or_else(|| {
            input_exception(
                stats_core::ErrorKind::InvalidInput,
                "each random term must be an array",
            )
        })?;
        let z = term.get("z").and_then(|v| v.array()).ok_or_else(|| {
            input_exception(
                stats_core::ErrorKind::InvalidInput,
                "each random term needs z",
            )
        })?;
        let groups = term.get("groups").and_then(|v| v.array()).ok_or_else(|| {
            input_exception(
                stats_core::ErrorKind::InvalidInput,
                "each random term needs groups",
            )
        })?;
        let z = numeric_rows(z)?;
        let groups = labels(groups)?;
        if z.len() != n
            || groups.len() != n
            || z[0].is_empty()
            || z.iter().any(|r| r.len() != z[0].len())
        {
            return Err(input_exception(
                stats_core::ErrorKind::DimensionMismatch,
                "each random z must be rectangular with one row and group per original observation",
            ));
        }
        let correlated = match term.get("correlated") {
            Some(v) => v.bool().ok_or_else(|| {
                input_exception(
                    stats_core::ErrorKind::InvalidInput,
                    "correlated must be a boolean",
                )
            })?,
            None => true,
        };
        terms.push((z, groups, correlated));
    }
    if terms.is_empty() {
        return Err(input_exception(
            stats_core::ErrorKind::InvalidInput,
            "supply at least one random term",
        ));
    }
    let mut retained = Vec::new();
    let mut dropped = Vec::new();
    for i in 0..n {
        if y[i].is_none()
            || weights[i].is_none()
            || offset[i].is_none()
            || x[i].iter().any(Option::is_none)
            || terms
                .iter()
                .any(|(z, g, _)| g[i].is_none() || z[i].iter().any(Option::is_none))
        {
            dropped.push(i as i64);
        } else {
            retained.push(i as i64);
        }
    }
    if missing == "raise" && !dropped.is_empty() {
        return Err(input_exception(
            stats_core::ErrorKind::InvalidInput,
            "missing values found with missing='raise'",
        ));
    }
    let mut term_labels = Vec::new();
    let core_terms = terms
        .into_iter()
        .map(|(z, g, correlated)| {
            let mut group_labels = Vec::new();
            let mut map = std::collections::HashMap::new();
            let groups = retained
                .iter()
                .map(|i| {
                    let label = g[*i as usize].as_ref().ok_or_else(|| {
                        input_exception(
                            stats_core::ErrorKind::Internal,
                            "missing group in retained row",
                        )
                    })?;
                    Ok(*map.entry(label.clone()).or_insert_with(|| {
                        group_labels.push(label.clone());
                        group_labels.len() - 1
                    }))
                })
                .collect::<PhpResult<Vec<_>>>()?;
            term_labels.push(group_labels);
            Ok(stats_core::RandomTerm {
                z: retained
                    .iter()
                    .map(|i| z[*i as usize].iter().flatten().copied().collect())
                    .collect(),
                groups,
                correlated,
            })
        })
        .collect::<PhpResult<Vec<_>>>()?;
    let select = |v: &[Option<f64>]| {
        retained
            .iter()
            .map(|i| {
                v[*i as usize].ok_or_else(|| {
                    input_exception(
                        stats_core::ErrorKind::Internal,
                        "missing value in retained row",
                    )
                })
            })
            .collect::<PhpResult<Vec<_>>>()
    };
    let start = start
        .map(|v| {
            v.iter()
                .map(|(_, v)| {
                    number(v)?.ok_or_else(|| {
                        input_exception(
                            stats_core::ErrorKind::InvalidInput,
                            "start cannot contain missing values",
                        )
                    })
                })
                .collect::<PhpResult<Vec<_>>>()
        })
        .transpose()?;
    let optimizer = match optimizer {
        "bobyqa" => stats_core::LmmOptimizer::Bobyqa,
        "nelder_mead" => stats_core::LmmOptimizer::NelderMead,
        _ => {
            return Err(input_exception(
                stats_core::ErrorKind::Unsupported,
                "optimizer must be 'bobyqa' or 'nelder_mead'; R wrapper names are not runtime aliases",
            ));
        }
    };
    let options = stats_core::LmmOptions {
        reml,
        optimizer,
        objective_tolerance: objectiveTolerance,
        weights: select(&weights)?,
        offset: select(&offset)?,
        start,
        max_evaluations: usize::try_from(maxEvaluations).map_err(|_| {
            input_exception(
                stats_core::ErrorKind::InvalidInput,
                "maxEvaluations must be positive",
            )
        })?,
        tolerance,
    };
    let result = stats_core::fit_lmm(
        fixed_design(
            retained
                .iter()
                .map(|i| x[*i as usize].iter().flatten().copied().collect())
                .collect(),
            intercept,
        ),
        select(&y)?,
        core_terms,
        options,
    )
    .map_err(stats_exception)?;
    let out = lmm_output(result, reml, term_labels)?;
    finish_result(out, n, retained, dropped, "Phit\\fit_mixed")
}

#[php_function]
#[php(
    name = "Phit\\fit_logistic_mixed",
    defaults(
        missing = "\"drop\"",
        quadraturePoints = 1,
        maxEvaluations = 20000,
        tolerance = 1e-7,
        intercept = true
    )
)]
// Preserve the procedural PHP signature and named arguments.
#[allow(clippy::too_many_arguments)]
pub fn fit_logistic_mixed(
    x: &ZendHashTable,
    y: &ZendHashTable,
    groups: &ZendHashTable,
    missing: &str,
    weights: Option<&ZendHashTable>,
    offset: Option<&ZendHashTable>,
    quadraturePoints: i64,
    maxEvaluations: i64,
    tolerance: f64,
    intercept: bool,
) -> PhpResult<ZBox<ZendHashTable>> {
    if let Some(weights) = weights {
        for (_, value) in weights.iter() {
            if number(value)?.is_some_and(|v| v > stats_core::MAX_LOGISTIC_MIXED_TRIALS) {
                return Err(input_exception(
                    stats_core::ErrorKind::Unsupported,
                    "trial counts must not exceed 1000000",
                ));
            }
        }
    }
    let input = prepare_data(
        x,
        y,
        missing,
        "binary",
        Some(groups),
        weights,
        offset,
        intercept,
    )?;
    let mut group_labels = Vec::new();
    let mut map = std::collections::HashMap::new();
    let indices = input
        .groups
        .iter()
        .map(|label| {
            *map.entry(label.clone()).or_insert_with(|| {
                group_labels.push(label.clone());
                group_labels.len() - 1
            })
        })
        .collect();
    let options = stats_core::LogisticMixedOptions {
        weights: input.weights,
        offset: input.offset,
        quadrature_points: usize::try_from(quadraturePoints).map_err(|_| {
            input_exception(
                stats_core::ErrorKind::Unsupported,
                "quadraturePoints must be 1..61",
            )
        })?,
        max_evaluations: usize::try_from(maxEvaluations).map_err(|_| {
            input_exception(
                stats_core::ErrorKind::InvalidInput,
                "maxEvaluations must be positive",
            )
        })?,
        tolerance,
    };
    let r = stats_core::fit_logistic_random_intercept(input.x, input.y, indices, options)
        .map_err(stats_exception)?;
    let out = logistic_mixed_output(r, &group_labels)?;
    finish_result(
        out,
        input.n_input,
        input.retained_rows,
        input.dropped_rows,
        "Phit\\fit_logistic_mixed",
    )
}

#[php_module]
pub fn get_module(module: ModuleBuilder) -> ModuleBuilder {
    let module = module
        .name("phit") // Canonical PHP/PIE extension identifier: extension=phit, ext-phit.
        .function(wrap_function!(fit_ols))
        .function(wrap_function!(fit_logistic))
        .function(wrap_function!(fit_probit))
        .function(wrap_function!(fit_poisson))
        .function(wrap_function!(fit_mixed_intercept))
        .function(wrap_function!(fit_mixed))
        .function(wrap_function!(fit_logistic_mixed));
    let module = formula::register(module);
    let module = distributions::register(module);
    csv::register(draws::register(smoothing::register(descriptive::register(
        module,
    ))))
}
