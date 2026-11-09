//! Formula bindings for Gaussian ridge and its cross-validation path.
use crate::{formula, input_exception, stats_exception};
use ext_php_rs::{
    boxed::ZBox,
    prelude::*,
    types::{ZendHashTable, Zval},
};
use stats_core::{
    ErrorKind,
    formula::ModelDesign,
    ridge::{RidgeOptions, RidgeResult},
};
pub struct RidgeMap(ZBox<ZendHashTable>);
impl RidgeMap {
    fn empty() -> Self {
        Self(ZendHashTable::new())
    }
}
impl ext_php_rs::convert::FromZval<'_> for RidgeMap {
    const TYPE: ext_php_rs::flags::DataType = ext_php_rs::flags::DataType::Array;
    fn from_zval(v: &Zval) -> Option<Self> {
        v.array().map(|a| Self(a.to_owned()))
    }
}
impl ext_php_rs::convert::StubLiteral for RidgeMap {
    fn stub_literal(&self) -> String {
        "[]".into()
    }
}
fn invalid(message: &str) -> PhpException {
    input_exception(ErrorKind::InvalidInput, message)
}
fn numeric(value: &Zval) -> PhpResult<f64> {
    value
        .double()
        .or_else(|| value.long().map(|v| v as f64))
        .filter(|v| v.is_finite())
        .ok_or_else(|| invalid("ridge options must be finite numbers"))
}
fn matching(design: &ModelDesign, pattern: &str) -> PhpResult<Vec<usize>> {
    let found: Vec<_> = design
        .columns
        .iter()
        .enumerate()
        .filter_map(|(j, name)| {
            (name != "(Intercept)" && stats_core::ridge::glob_matches(pattern, name)).then_some(j)
        })
        .collect();
    if found.is_empty() {
        Err(invalid(&format!(
            "pattern '{pattern}' matches no predictor columns"
        )))
    } else {
        Ok(found)
    }
}
fn options(
    design: &ModelDesign,
    lambda: f64,
    penalty_factor: Option<&ZendHashTable>,
    standardize: bool,
    bounds: Option<&ZendHashTable>,
) -> PhpResult<RidgeOptions> {
    if !design.random.is_empty() || design.offset.is_some() {
        return Err(input_exception(
            ErrorKind::Unsupported,
            "ridge does not support random effects or offsets",
        ));
    }
    let intercept = design.columns.iter().position(|c| c == "(Intercept)");
    let mut factors = vec![1.; design.columns.len()];
    if let Some(j) = intercept {
        factors[j] = 0.;
    }
    if let Some(table) = penalty_factor {
        for (pattern, value) in table.iter() {
            let pattern = pattern.to_string();
            let value = numeric(value)?;
            for j in matching(design, &pattern)? {
                factors[j] = value;
            }
        }
    }
    let mut limits = vec![[None, None]; design.columns.len()];
    if let Some(table) = bounds {
        for (pattern, value) in table.iter() {
            let pattern = pattern.to_string();
            let array = value
                .array()
                .filter(|a| a.len() == 2)
                .ok_or_else(|| invalid("each bound must be [lower|null, upper|null]"))?;
            let b = array
                .values()
                .map(|v| {
                    if v.is_null() {
                        Ok(None)
                    } else {
                        numeric(v).map(Some)
                    }
                })
                .collect::<PhpResult<Vec<_>>>()?;
            for j in matching(design, &pattern)? {
                limits[j] = [b[0], b[1]];
            }
        }
    }
    Ok(RidgeOptions {
        lambda,
        penalty_factor: factors,
        standardize,
        bounds: limits,
        intercept,
    })
}
fn output(result: RidgeResult, design: &ModelDesign, options: &RidgeOptions) -> PhpResult<Zval> {
    let mut out = ZendHashTable::new();
    let distribution =
        stats_core::distributions::StudentT::new(result.df_residual).map_err(stats_exception)?;
    let statistics: Vec<_> = result
        .coefficients
        .iter()
        .zip(&result.std_errors)
        .map(|(b, s)| s.map(|s| b / s))
        .collect();
    let p_values: Vec<_> = statistics
        .iter()
        .map(|t| t.map(|t| 2. * distribution.sf(t.abs())))
        .collect();
    out.insert("coefficients", result.coefficients)?;
    out.insert("covariance", result.covariance)?;
    out.insert("std_errors", result.std_errors)?;
    out.insert("t_statistics", statistics)?;
    out.insert("p_values", p_values)?;
    out.insert(
        "conf_int",
        result
            .conf_int
            .into_iter()
            .map(|v| v.map(|v| v.to_vec()))
            .collect::<Vec<_>>(),
    )?;
    out.insert("fitted", result.fitted)?;
    out.insert("residuals", result.residuals)?;
    out.insert("lambda", options.lambda)?;
    out.insert("lambda_effective", result.lambda_effective)?;
    out.insert("response_scale", result.response_scale)?;
    out.insert("lambda_lm_ridge", result.lambda_lm_ridge)?;
    out.insert("penalty_factor", result.penalty_factor)?;
    out.insert("standardize", options.standardize)?;
    out.insert(
        "bounds",
        options
            .bounds
            .iter()
            .map(|b| b.to_vec())
            .collect::<Vec<_>>(),
    )?;
    out.insert("at_bound", result.at_bound)?;
    out.insert("df", result.df)?;
    out.insert("df_residual", result.df_residual)?;
    out.insert("rss", result.rss)?;
    out.insert("sigma", result.sigma)?;
    out.insert("r_squared", result.r_squared)?;
    out.insert("iterations", result.iterations as i64)?;
    formula::add_names(&mut out, design)?;
    let (retained, dropped) = formula::rows(design);
    crate::finish_result(out, design.n_input, retained, dropped, "Phit\\fit_ridge")
}
#[php_function]
#[php(
    name = "Phit\\fit_ridge",
    defaults(
        penaltyFactor = RidgeMap::empty(),
        standardize = true,
        bounds = RidgeMap::empty(),
        missing = "\"drop\""
    )
)]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn fit_ridge(
    formula: &str,
    data: &Zval,
    lambda: crate::distributions::Real,
    penaltyFactor: RidgeMap,
    standardize: bool,
    bounds: RidgeMap,
    missing: &str,
    weights: Option<&Zval>,
) -> PhpResult<Zval> {
    let prepared = formula::prepare(formula, data, missing, weights, true)?;
    let options = options(
        &prepared.design,
        lambda.0,
        Some(&penaltyFactor.0),
        standardize,
        Some(&bounds.0),
    )?;
    let y = formula::numeric_response(&prepared.design, "Phit\\fit_ridge")?;
    let result = stats_core::ridge::fit_ridge(&prepared.design.x, &y, &prepared.weights, &options)
        .map_err(stats_exception)?;
    output(result, &prepared.design, &options)
}
#[php_function]
#[php(
    name = "Phit\\cv_ridge",
    defaults(
        nfolds = 10,
        seed = 0,
        penaltyFactor = RidgeMap::empty(),
        standardize = true,
        bounds = RidgeMap::empty(),
        missing = "\"drop\""
    )
)]
#[allow(non_snake_case, clippy::too_many_arguments)]
pub fn cv_ridge(
    formula: &str,
    data: &Zval,
    lambda: Option<&ZendHashTable>,
    foldid: Option<&ZendHashTable>,
    nfolds: i64,
    seed: i64,
    penaltyFactor: RidgeMap,
    standardize: bool,
    bounds: RidgeMap,
    missing: &str,
    weights: Option<&Zval>,
) -> PhpResult<ZBox<ZendHashTable>> {
    let prepared = formula::prepare(formula, data, missing, weights, true)?;
    let options = options(
        &prepared.design,
        0.,
        Some(&penaltyFactor.0),
        standardize,
        Some(&bounds.0),
    )?;
    let y = formula::numeric_response(&prepared.design, "Phit\\cv_ridge")?;
    let lambda = match lambda {
        Some(v) => v.values().map(numeric).collect::<PhpResult<Vec<_>>>()?,
        None => {
            stats_core::ridge::default_lambda(&prepared.design.x, &y, &prepared.weights, &options)
                .map_err(stats_exception)?
        }
    };
    let n = prepared.design.x.len();
    let folds = if let Some(foldid) = foldid {
        if foldid.len() != prepared.design.n_input {
            return Err(input_exception(
                ErrorKind::DimensionMismatch,
                "foldid must have one label per original row",
            ));
        }
        let labels = foldid
            .values()
            .map(|v| {
                v.long()
                    .ok_or_else(|| invalid("foldid must contain integer labels"))
            })
            .collect::<PhpResult<Vec<_>>>()?;
        let mut distinct = labels.clone();
        distinct.sort_unstable();
        distinct.dedup();
        prepared
            .design
            .retained_rows
            .iter()
            .map(|i| distinct.binary_search(&labels[*i]).unwrap())
            .collect::<Vec<_>>()
    } else {
        if seed < 0 || nfolds < 3 || nfolds as usize > n {
            return Err(invalid(
                "seed must be nonnegative and nfolds between 3 and the retained row count",
            ));
        }
        let order = stats_core::distributions::sample_permutation(n, seed as u64, n)
            .map_err(stats_exception)?;
        let mut labels = vec![0; n];
        for (j, i) in order.iter().enumerate() {
            labels[*i] = j % nfolds as usize;
        }
        labels
    };
    let result = stats_core::ridge::cv_ridge(
        &prepared.design.x,
        &y,
        &prepared.weights,
        &options,
        &lambda,
        &folds,
    )
    .map_err(stats_exception)?;
    let mut out = ZendHashTable::new();
    out.insert("lambda", result.lambda.clone())?;
    out.insert("cvm", result.cvm.clone())?;
    out.insert("cvsd", result.cvsd.clone())?;
    out.insert(
        "cvup",
        result
            .cvm
            .iter()
            .zip(&result.cvsd)
            .map(|(m, s)| m + s)
            .collect::<Vec<_>>(),
    )?;
    out.insert(
        "cvlo",
        result
            .cvm
            .iter()
            .zip(&result.cvsd)
            .map(|(m, s)| m - s)
            .collect::<Vec<_>>(),
    )?;
    out.insert("lambda_min", result.lambda[result.index_min])?;
    out.insert("lambda_1se", result.lambda[result.index_1se])?;
    out.insert("index_min", result.index_min as i64)?;
    out.insert("index_1se", result.index_1se as i64)?;
    out.insert(
        "fit",
        output(
            result.fit,
            &prepared.design,
            &RidgeOptions {
                lambda: result.lambda[result.index_min],
                ..options
            },
        )?,
    )?;
    Ok(out)
}
pub fn register(module: ModuleBuilder) -> ModuleBuilder {
    module
        .function(wrap_function!(fit_ridge))
        .function(wrap_function!(cv_ridge))
}
