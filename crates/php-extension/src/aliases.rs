//! The 0.4 names, transition: Phit\fit_ols dispatches between the formula fit and the deprecated
//! matrix fit, and the 0.3 names are deprecated aliases that forward their arguments unchanged.
//! Each alias raises one E_USER_DEPRECATED per call naming its replacement. Phit 0.5 removes the
//! aliases and the matrix form of fit_ols.

use crate::formula::{fit_generalized_linear, fit_mixed_gaussian, fit_mixed_logistic, ols_formula};
use crate::{
    fit_logistic_matrix, fit_mixed_gaussian_intercept_matrix, fit_mixed_gaussian_matrix,
    fit_mixed_logistic_matrix, fit_ols_matrix, fit_poisson_matrix, fit_probit_matrix,
    input_exception,
};
use ext_php_rs::{
    boxed::ZBox,
    prelude::*,
    types::{ZendHashTable, Zval},
};
use stats_core::ErrorKind;

fn deprecated(message: &str) {
    ext_php_rs::error::php_error(&ext_php_rs::flags::ErrorType::UserDeprecated, message);
}

// PHP prefixes the message with the called function, "Phit\\fit_lm(): ".
fn alias(new: &str) {
    deprecated(&format!(
        "Deprecated since phit 0.4.0; use Phit\\{new}() instead"
    ));
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

#[php_function]
#[php(name = "Phit\\fit_lm", defaults(missing = "\"drop\""))]
pub fn fit_lm(
    formula: &str,
    data: &Zval,
    missing: &str,
    weights: Option<&Zval>,
) -> PhpResult<ZBox<ZendHashTable>> {
    alias("fit_ols");
    ols_formula(formula, data, missing, weights)
}

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
    alias("fit_generalized_linear");
    fit_generalized_linear(formula, data, family, link, missing, weights)
}

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
#[allow(clippy::too_many_arguments)]
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
    alias("fit_mixed_gaussian");
    fit_mixed_gaussian(
        formula,
        data,
        reml,
        missing,
        weights,
        start,
        maxEvaluations,
        tolerance,
        optimizer,
        objectiveTolerance,
    )
}

/// The 0.3 signature keeps family and link; Phit\fit_mixed_logistic fits only the binomial logit.
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
#[allow(clippy::too_many_arguments)]
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
    alias("fit_mixed_logistic");
    if family != "binomial" || !matches!(link, "" | "logit") {
        return Err(input_exception(
            ErrorKind::Unsupported,
            &format!(
                "Phit\\fit_glmer supports family 'binomial' with the logit link only (got '{family}'/'{link}')"
            ),
        ));
    }
    fit_mixed_logistic(
        formula,
        data,
        missing,
        weights,
        quadraturePoints,
        maxEvaluations,
        tolerance,
    )
}

macro_rules! glm_matrix_alias {
    ($old:ident, $name:literal, $new:ident) => {
        #[php_function]
        #[php(name = $name, defaults(missing = "\"drop\"", intercept = true))]
        pub fn $old(
            x: &ZendHashTable,
            y: &ZendHashTable,
            missing: &str,
            weights: Option<&ZendHashTable>,
            offset: Option<&ZendHashTable>,
            intercept: bool,
        ) -> PhpResult<ZBox<ZendHashTable>> {
            alias(stringify!($new));
            $new(x, y, missing, weights, offset, intercept)
        }
    };
}

glm_matrix_alias!(fit_logistic, "Phit\\fit_logistic", fit_logistic_matrix);
glm_matrix_alias!(fit_probit, "Phit\\fit_probit", fit_probit_matrix);
glm_matrix_alias!(fit_poisson, "Phit\\fit_poisson", fit_poisson_matrix);

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
    alias("fit_mixed_gaussian_intercept_matrix");
    fit_mixed_gaussian_intercept_matrix(x, y, groups, missing, intercept)
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
    alias("fit_mixed_gaussian_matrix");
    fit_mixed_gaussian_matrix(
        x,
        y,
        random,
        missing,
        reml,
        weights,
        offset,
        start,
        maxEvaluations,
        tolerance,
        optimizer,
        objectiveTolerance,
        intercept,
    )
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
    alias("fit_mixed_logistic_matrix");
    fit_mixed_logistic_matrix(
        x,
        y,
        groups,
        missing,
        weights,
        offset,
        quadraturePoints,
        maxEvaluations,
        tolerance,
        intercept,
    )
}

pub fn register(module: ModuleBuilder) -> ModuleBuilder {
    module
        .function(wrap_function!(fit_ols))
        .function(wrap_function!(fit_lm))
        .function(wrap_function!(fit_glm))
        .function(wrap_function!(fit_lmer))
        .function(wrap_function!(fit_glmer))
        .function(wrap_function!(fit_logistic))
        .function(wrap_function!(fit_probit))
        .function(wrap_function!(fit_poisson))
        .function(wrap_function!(fit_mixed_intercept))
        .function(wrap_function!(fit_mixed))
        .function(wrap_function!(fit_logistic_mixed))
}
