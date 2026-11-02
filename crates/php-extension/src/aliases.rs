//! Phit\fit_ols dispatches between the formula fit and the deprecated matrix fit, which raises one
//! E_USER_DEPRECATED per call naming Phit\fit_ols_matrix. The 0.3 aliases were removed in 0.5.0.

use crate::formula::ols_formula;
use crate::{fit_ols_matrix, input_exception};
use ext_php_rs::{
    boxed::ZBox,
    prelude::*,
    types::{ZendHashTable, Zval},
};
use stats_core::ErrorKind;

fn deprecated(message: &str) {
    ext_php_rs::error::php_error(&ext_php_rs::flags::ErrorType::UserDeprecated, message);
}

fn invalid(message: &str) -> PhpException {
    input_exception(ErrorKind::InvalidInput, message)
}

fn given(value: Option<&Zval>) -> Option<&Zval> {
    value.filter(|v| !v.is_null())
}

/// Linear model by least squares. A string first argument (or named formula:/data:) is the
/// formula fit. An array first argument (or named x:/y:) is the deprecated matrix fit, which
/// forwards to Phit\fit_ols_matrix.
#[php_function]
#[php(
    name = "Phit\\fit_ols",
    defaults(missing = "\"drop\"", intercept = true)
)]
#[allow(clippy::too_many_arguments)] // One signature serves both forms during the transition.
pub fn fit_ols(
    formula: Option<&Zval>,
    data: Option<&Zval>,
    missing: &str,
    weights: Option<&Zval>,
    offset: Option<&ZendHashTable>,
    intercept: bool,
    x: Option<&ZendHashTable>,
    y: Option<&ZendHashTable>,
) -> PhpResult<ZBox<ZendHashTable>> {
    let (formula, data, weights) = (given(formula), given(data), given(weights));
    if let Some(text) = formula.and_then(|f| f.str()) {
        if x.is_some() || y.is_some() {
            return Err(invalid(
                "Phit\\fit_ols takes a formula and data, or a matrix x and y, not both",
            ));
        }
        if offset.is_some() || !intercept {
            return Err(invalid(
                "Phit\\fit_ols with a formula takes no offset or intercept arguments; write offset() or - 1 in the formula",
            ));
        }
        let data = data.ok_or_else(|| invalid("Phit\\fit_ols with a formula needs data"))?;
        return ols_formula(text, data, missing, weights);
    }
    if formula.is_some_and(|f| f.array().is_none()) {
        return Err(invalid(
            "Phit\\fit_ols needs a formula string, or a matrix x as an array",
        ));
    }
    if formula.is_some() && x.is_some() || data.is_some() && y.is_some() {
        return Err(invalid(
            "Phit\\fit_ols takes a formula and data, or a matrix x and y, not both",
        ));
    }
    let x = x.or_else(|| formula.and_then(|f| f.array()));
    let y = match (y, data) {
        (Some(y), _) => Some(y),
        (None, Some(d)) => Some(
            d.array()
                .ok_or_else(|| invalid("Phit\\fit_ols with a matrix x needs y as an array"))?,
        ),
        (None, None) => None,
    };
    let (Some(x), Some(y)) = (x, y) else {
        return Err(invalid(
            "Phit\\fit_ols needs a formula and data (or, deprecated, a matrix x and y)",
        ));
    };
    let weights =
        match weights {
            None => None,
            Some(w) => Some(w.array().ok_or_else(|| {
                invalid("Phit\\fit_ols with a matrix x needs weights as an array")
            })?),
        };
    deprecated(
        "Passing a matrix is deprecated since phit 0.4.0; use Phit\\fit_ols_matrix() instead",
    );
    fit_ols_matrix(x, y, missing, weights, offset, intercept)
}

pub fn register(module: ModuleBuilder) -> ModuleBuilder {
    module.function(wrap_function!(fit_ols))
}
