//! PHP bindings for probability distributions and descriptive statistics.
//!
//! Every operation has a scalar function and an `_array` batch function. Both
//! construct the distribution once, apply the same validation through
//! stats_core::distributions::{evaluate, evaluate_quantile}, and differ only in
//! iterating a PHP array natively instead of crossing the PHP boundary per value.
use crate::stats_exception;
use ext_php_rs::{
    boxed::ZBox, convert::FromZval, convert::StubLiteral, flags::DataType, prelude::*,
    types::ZendHashTable, types::Zval,
};
use stats_core::distributions::{self as dist, evaluate, evaluate_quantile};
use stats_core::{ErrorKind, StatsError};

/// A PHP int or float argument. ext-php-rs' f64 accepts floats only; PHP
/// callers routinely pass integers such as degrees of freedom.
#[derive(Clone, Copy)]
pub struct Real(pub f64);
impl FromZval<'_> for Real {
    const TYPE: DataType = DataType::Double;
    fn from_zval(zval: &Zval) -> Option<Self> {
        zval.double()
            .or_else(|| zval.long().map(|v| v as f64))
            .map(Real)
    }
}
impl From<f64> for Real {
    fn from(value: f64) -> Self {
        Real(value)
    }
}
impl StubLiteral for Real {
    fn stub_literal(&self) -> String {
        self.0.stub_literal()
    }
}

fn error(kind: ErrorKind, message: &str) -> PhpException {
    stats_exception(StatsError::new(kind, message))
}

// Array elements must be PHP ints or floats; there is no missing-value policy.
fn element(value: &Zval) -> PhpResult<f64> {
    value
        .double()
        .or_else(|| value.long().map(|v| v as f64))
        .ok_or_else(|| {
            error(
                ErrorKind::InvalidInput,
                "array elements must be integers or floats",
            )
        })
}

/// Apply f to every element, preserving iteration order; keys are not kept.
fn map_array(
    values: &ZendHashTable,
    f: impl Fn(f64) -> Result<f64, StatsError>,
) -> PhpResult<ZBox<ZendHashTable>> {
    let capacity = u32::try_from(values.len())
        .map_err(|_| error(ErrorKind::Unsupported, "array is too large"))?;
    let mut out = ZendHashTable::with_capacity(capacity);
    for value in values.values() {
        out.push(f(element(value)?).map_err(stats_exception)?)?;
    }
    Ok(out)
}

fn trials(value: i64) -> Result<u64, StatsError> {
    u64::try_from(value)
        .map_err(|_| StatsError::new(ErrorKind::InvalidInput, "size must be nonnegative"))
}

// One scalar/batch pair for one operation.
macro_rules! pair {
    (
        $(#[$attr:meta])*
        $scalar:ident = $scalar_name:literal, $batch:ident = $batch_name:literal,
        $point:ident, $points:ident, $check:ident, $method:ident,
        ($($param:ident: $ty:ty),*) => $build:expr
    ) => {
        #[php_function]
        #[php(name = $scalar_name)]
        $(#[$attr])*
        pub fn $scalar($point: Real, $($param: $ty),*) -> PhpResult<f64> {
            let distribution = $build.map_err(stats_exception)?;
            $check($point.0, |v| distribution.$method(v)).map_err(stats_exception)
        }
        #[php_function]
        #[php(name = $batch_name)]
        $(#[$attr])*
        pub fn $batch($points: &ZendHashTable, $($param: $ty),*) -> PhpResult<ZBox<ZendHashTable>> {
            let distribution = $build.map_err(stats_exception)?;
            map_array($points, |v| $check(v, |v| distribution.$method(v)))
        }
    };
}

// Seven evaluation operations and explicitly seeded batch sampling.
macro_rules! distribution {
    (
        $(#[$attr:meta])*
        register: $register:ident,
        rng: $f_rng:ident = $n_rng:literal,
        point: $point:ident, $points:ident, density: $density:ident, $log_density:ident,
        params: ($($param:ident: $ty:ty),*) => $build:expr,
        functions: [
            $f_density:ident = $n_density:literal, $f_density_array:ident = $n_density_array:literal, $f_log_density:ident = $n_log_density:literal, $f_log_density_array:ident = $n_log_density_array:literal,
            $f_cdf:ident = $n_cdf:literal, $f_cdf_array:ident = $n_cdf_array:literal, $f_logcdf:ident = $n_logcdf:literal, $f_logcdf_array:ident = $n_logcdf_array:literal,
            $f_sf:ident = $n_sf:literal, $f_sf_array:ident = $n_sf_array:literal, $f_logsf:ident = $n_logsf:literal, $f_logsf_array:ident = $n_logsf_array:literal,
            $f_quantile:ident = $n_quantile:literal, $f_quantile_array:ident = $n_quantile_array:literal
        ]
    ) => {
        pair!($(#[$attr])* $f_density = $n_density, $f_density_array = $n_density_array, $point, $points, evaluate, $density, ($($param: $ty),*) => $build);
        pair!($(#[$attr])* $f_log_density = $n_log_density, $f_log_density_array = $n_log_density_array, $point, $points, evaluate, $log_density, ($($param: $ty),*) => $build);
        pair!($(#[$attr])* $f_cdf = $n_cdf, $f_cdf_array = $n_cdf_array, $point, $points, evaluate, cdf, ($($param: $ty),*) => $build);
        pair!($(#[$attr])* $f_logcdf = $n_logcdf, $f_logcdf_array = $n_logcdf_array, $point, $points, evaluate, logcdf, ($($param: $ty),*) => $build);
        pair!($(#[$attr])* $f_sf = $n_sf, $f_sf_array = $n_sf_array, $point, $points, evaluate, sf, ($($param: $ty),*) => $build);
        pair!($(#[$attr])* $f_logsf = $n_logsf, $f_logsf_array = $n_logsf_array, $point, $points, evaluate, logsf, ($($param: $ty),*) => $build);
        pair!($(#[$attr])* $f_quantile = $n_quantile, $f_quantile_array = $n_quantile_array, p, probabilities, evaluate_quantile, quantile, ($($param: $ty),*) => $build);
        #[php_function]
        #[php(name = $n_rng)]
        $(#[$attr])*
        pub fn $f_rng(n: i64, seed: i64, $($param: $ty),*) -> PhpResult<Vec<f64>> {
            let distribution = $build.map_err(stats_exception)?;
            let n = usize::try_from(n).map_err(|_| error(ErrorKind::InvalidInput, "n must be nonnegative"))?;
            let seed = u64::try_from(seed).map_err(|_| error(ErrorKind::InvalidInput, "seed must be nonnegative"))?;
            dist::sample_quantiles(n, seed, |p| distribution.quantile(p)).map_err(stats_exception)
        }
        fn $register(module: ModuleBuilder) -> ModuleBuilder {
            module
                .function(wrap_function!($f_density))
                .function(wrap_function!($f_density_array))
                .function(wrap_function!($f_log_density))
                .function(wrap_function!($f_log_density_array))
                .function(wrap_function!($f_cdf))
                .function(wrap_function!($f_cdf_array))
                .function(wrap_function!($f_logcdf))
                .function(wrap_function!($f_logcdf_array))
                .function(wrap_function!($f_sf))
                .function(wrap_function!($f_sf_array))
                .function(wrap_function!($f_logsf))
                .function(wrap_function!($f_logsf_array))
                .function(wrap_function!($f_quantile))
                .function(wrap_function!($f_quantile_array))
                .function(wrap_function!($f_rng))
        }
    };
}

distribution!(
    #[php(defaults(mean = 0.0, sd = 1.0))]
    register: register_normal,
    rng: dist_normal_rng = "Phit\\dist_normal_rng",
    point: x, values, density: pdf, logpdf,
    params: (mean: Real, sd: Real) => dist::Normal::new(mean.0, sd.0),
    functions: [
        dist_normal_pdf = "Phit\\dist_normal_pdf", dist_normal_pdf_array = "Phit\\dist_normal_pdf_array", dist_normal_logpdf = "Phit\\dist_normal_logpdf", dist_normal_logpdf_array = "Phit\\dist_normal_logpdf_array",
        dist_normal_cdf = "Phit\\dist_normal_cdf", dist_normal_cdf_array = "Phit\\dist_normal_cdf_array", dist_normal_logcdf = "Phit\\dist_normal_logcdf", dist_normal_logcdf_array = "Phit\\dist_normal_logcdf_array",
        dist_normal_sf = "Phit\\dist_normal_sf", dist_normal_sf_array = "Phit\\dist_normal_sf_array", dist_normal_logsf = "Phit\\dist_normal_logsf", dist_normal_logsf_array = "Phit\\dist_normal_logsf_array",
        dist_normal_quantile = "Phit\\dist_normal_quantile", dist_normal_quantile_array = "Phit\\dist_normal_quantile_array"
    ]
);
distribution!(
    register: register_t,
    rng: dist_t_rng = "Phit\\dist_t_rng",
    point: x, values, density: pdf, logpdf,
    params: (df: Real) => dist::StudentT::new(df.0),
    functions: [
        dist_t_pdf = "Phit\\dist_t_pdf", dist_t_pdf_array = "Phit\\dist_t_pdf_array", dist_t_logpdf = "Phit\\dist_t_logpdf", dist_t_logpdf_array = "Phit\\dist_t_logpdf_array",
        dist_t_cdf = "Phit\\dist_t_cdf", dist_t_cdf_array = "Phit\\dist_t_cdf_array", dist_t_logcdf = "Phit\\dist_t_logcdf", dist_t_logcdf_array = "Phit\\dist_t_logcdf_array",
        dist_t_sf = "Phit\\dist_t_sf", dist_t_sf_array = "Phit\\dist_t_sf_array", dist_t_logsf = "Phit\\dist_t_logsf", dist_t_logsf_array = "Phit\\dist_t_logsf_array",
        dist_t_quantile = "Phit\\dist_t_quantile", dist_t_quantile_array = "Phit\\dist_t_quantile_array"
    ]
);
distribution!(
    register: register_chisquare,
    rng: dist_chisquare_rng = "Phit\\dist_chisquare_rng",
    point: x, values, density: pdf, logpdf,
    params: (df: Real) => dist::ChiSquare::new(df.0),
    functions: [
        dist_chisquare_pdf = "Phit\\dist_chisquare_pdf", dist_chisquare_pdf_array = "Phit\\dist_chisquare_pdf_array", dist_chisquare_logpdf = "Phit\\dist_chisquare_logpdf", dist_chisquare_logpdf_array = "Phit\\dist_chisquare_logpdf_array",
        dist_chisquare_cdf = "Phit\\dist_chisquare_cdf", dist_chisquare_cdf_array = "Phit\\dist_chisquare_cdf_array", dist_chisquare_logcdf = "Phit\\dist_chisquare_logcdf", dist_chisquare_logcdf_array = "Phit\\dist_chisquare_logcdf_array",
        dist_chisquare_sf = "Phit\\dist_chisquare_sf", dist_chisquare_sf_array = "Phit\\dist_chisquare_sf_array", dist_chisquare_logsf = "Phit\\dist_chisquare_logsf", dist_chisquare_logsf_array = "Phit\\dist_chisquare_logsf_array",
        dist_chisquare_quantile = "Phit\\dist_chisquare_quantile", dist_chisquare_quantile_array = "Phit\\dist_chisquare_quantile_array"
    ]
);
distribution!(
    register: register_f,
    rng: dist_f_rng = "Phit\\dist_f_rng",
    point: x, values, density: pdf, logpdf,
    params: (df1: Real, df2: Real) => dist::FisherF::new(df1.0, df2.0),
    functions: [
        dist_f_pdf = "Phit\\dist_f_pdf", dist_f_pdf_array = "Phit\\dist_f_pdf_array", dist_f_logpdf = "Phit\\dist_f_logpdf", dist_f_logpdf_array = "Phit\\dist_f_logpdf_array",
        dist_f_cdf = "Phit\\dist_f_cdf", dist_f_cdf_array = "Phit\\dist_f_cdf_array", dist_f_logcdf = "Phit\\dist_f_logcdf", dist_f_logcdf_array = "Phit\\dist_f_logcdf_array",
        dist_f_sf = "Phit\\dist_f_sf", dist_f_sf_array = "Phit\\dist_f_sf_array", dist_f_logsf = "Phit\\dist_f_logsf", dist_f_logsf_array = "Phit\\dist_f_logsf_array",
        dist_f_quantile = "Phit\\dist_f_quantile", dist_f_quantile_array = "Phit\\dist_f_quantile_array"
    ]
);
distribution!(
    register: register_beta,
    rng: dist_beta_rng = "Phit\\dist_beta_rng",
    point: x, values, density: pdf, logpdf,
    params: (alpha: Real, beta: Real) => dist::Beta::new(alpha.0, beta.0),
    functions: [
        dist_beta_pdf = "Phit\\dist_beta_pdf", dist_beta_pdf_array = "Phit\\dist_beta_pdf_array", dist_beta_logpdf = "Phit\\dist_beta_logpdf", dist_beta_logpdf_array = "Phit\\dist_beta_logpdf_array",
        dist_beta_cdf = "Phit\\dist_beta_cdf", dist_beta_cdf_array = "Phit\\dist_beta_cdf_array", dist_beta_logcdf = "Phit\\dist_beta_logcdf", dist_beta_logcdf_array = "Phit\\dist_beta_logcdf_array",
        dist_beta_sf = "Phit\\dist_beta_sf", dist_beta_sf_array = "Phit\\dist_beta_sf_array", dist_beta_logsf = "Phit\\dist_beta_logsf", dist_beta_logsf_array = "Phit\\dist_beta_logsf_array",
        dist_beta_quantile = "Phit\\dist_beta_quantile", dist_beta_quantile_array = "Phit\\dist_beta_quantile_array"
    ]
);
distribution!(
    #[php(defaults(scale = 1.0))]
    register: register_gamma,
    rng: dist_gamma_rng = "Phit\\dist_gamma_rng",
    point: x, values, density: pdf, logpdf,
    params: (shape: Real, scale: Real) => dist::Gamma::new(shape.0, scale.0),
    functions: [
        dist_gamma_pdf = "Phit\\dist_gamma_pdf", dist_gamma_pdf_array = "Phit\\dist_gamma_pdf_array", dist_gamma_logpdf = "Phit\\dist_gamma_logpdf", dist_gamma_logpdf_array = "Phit\\dist_gamma_logpdf_array",
        dist_gamma_cdf = "Phit\\dist_gamma_cdf", dist_gamma_cdf_array = "Phit\\dist_gamma_cdf_array", dist_gamma_logcdf = "Phit\\dist_gamma_logcdf", dist_gamma_logcdf_array = "Phit\\dist_gamma_logcdf_array",
        dist_gamma_sf = "Phit\\dist_gamma_sf", dist_gamma_sf_array = "Phit\\dist_gamma_sf_array", dist_gamma_logsf = "Phit\\dist_gamma_logsf", dist_gamma_logsf_array = "Phit\\dist_gamma_logsf_array",
        dist_gamma_quantile = "Phit\\dist_gamma_quantile", dist_gamma_quantile_array = "Phit\\dist_gamma_quantile_array"
    ]
);
distribution!(
    #[php(defaults(rate = 1.0))]
    register: register_exponential,
    rng: dist_exponential_rng = "Phit\\dist_exponential_rng",
    point: x, values, density: pdf, logpdf,
    params: (rate: Real) => dist::Exponential::new(rate.0),
    functions: [
        dist_exponential_pdf = "Phit\\dist_exponential_pdf", dist_exponential_pdf_array = "Phit\\dist_exponential_pdf_array", dist_exponential_logpdf = "Phit\\dist_exponential_logpdf", dist_exponential_logpdf_array = "Phit\\dist_exponential_logpdf_array",
        dist_exponential_cdf = "Phit\\dist_exponential_cdf", dist_exponential_cdf_array = "Phit\\dist_exponential_cdf_array", dist_exponential_logcdf = "Phit\\dist_exponential_logcdf", dist_exponential_logcdf_array = "Phit\\dist_exponential_logcdf_array",
        dist_exponential_sf = "Phit\\dist_exponential_sf", dist_exponential_sf_array = "Phit\\dist_exponential_sf_array", dist_exponential_logsf = "Phit\\dist_exponential_logsf", dist_exponential_logsf_array = "Phit\\dist_exponential_logsf_array",
        dist_exponential_quantile = "Phit\\dist_exponential_quantile", dist_exponential_quantile_array = "Phit\\dist_exponential_quantile_array"
    ]
);
distribution!(
    #[php(defaults(meanlog = 0.0, sdlog = 1.0))]
    register: register_lognormal,
    rng: dist_lognormal_rng = "Phit\\dist_lognormal_rng",
    point: x, values, density: pdf, logpdf,
    params: (meanlog: Real, sdlog: Real) => dist::LogNormal::new(meanlog.0, sdlog.0),
    functions: [
        dist_lognormal_pdf = "Phit\\dist_lognormal_pdf", dist_lognormal_pdf_array = "Phit\\dist_lognormal_pdf_array", dist_lognormal_logpdf = "Phit\\dist_lognormal_logpdf", dist_lognormal_logpdf_array = "Phit\\dist_lognormal_logpdf_array",
        dist_lognormal_cdf = "Phit\\dist_lognormal_cdf", dist_lognormal_cdf_array = "Phit\\dist_lognormal_cdf_array", dist_lognormal_logcdf = "Phit\\dist_lognormal_logcdf", dist_lognormal_logcdf_array = "Phit\\dist_lognormal_logcdf_array",
        dist_lognormal_sf = "Phit\\dist_lognormal_sf", dist_lognormal_sf_array = "Phit\\dist_lognormal_sf_array", dist_lognormal_logsf = "Phit\\dist_lognormal_logsf", dist_lognormal_logsf_array = "Phit\\dist_lognormal_logsf_array",
        dist_lognormal_quantile = "Phit\\dist_lognormal_quantile", dist_lognormal_quantile_array = "Phit\\dist_lognormal_quantile_array"
    ]
);
distribution!(
    #[php(defaults(location = 0.0, scale = 1.0))]
    register: register_logistic,
    rng: dist_logistic_rng = "Phit\\dist_logistic_rng",
    point: x, values, density: pdf, logpdf,
    params: (location: Real, scale: Real) => dist::Logistic::new(location.0, scale.0),
    functions: [
        dist_logistic_pdf = "Phit\\dist_logistic_pdf", dist_logistic_pdf_array = "Phit\\dist_logistic_pdf_array", dist_logistic_logpdf = "Phit\\dist_logistic_logpdf", dist_logistic_logpdf_array = "Phit\\dist_logistic_logpdf_array",
        dist_logistic_cdf = "Phit\\dist_logistic_cdf", dist_logistic_cdf_array = "Phit\\dist_logistic_cdf_array", dist_logistic_logcdf = "Phit\\dist_logistic_logcdf", dist_logistic_logcdf_array = "Phit\\dist_logistic_logcdf_array",
        dist_logistic_sf = "Phit\\dist_logistic_sf", dist_logistic_sf_array = "Phit\\dist_logistic_sf_array", dist_logistic_logsf = "Phit\\dist_logistic_logsf", dist_logistic_logsf_array = "Phit\\dist_logistic_logsf_array",
        dist_logistic_quantile = "Phit\\dist_logistic_quantile", dist_logistic_quantile_array = "Phit\\dist_logistic_quantile_array"
    ]
);
distribution!(
    register: register_binomial,
    rng: dist_binomial_rng = "Phit\\dist_binomial_rng",
    point: k, counts, density: pmf, logpmf,
    params: (size: i64, prob: Real) => trials(size).and_then(|n| dist::Binomial::new(n, prob.0)),
    functions: [
        dist_binomial_pmf = "Phit\\dist_binomial_pmf", dist_binomial_pmf_array = "Phit\\dist_binomial_pmf_array", dist_binomial_logpmf = "Phit\\dist_binomial_logpmf", dist_binomial_logpmf_array = "Phit\\dist_binomial_logpmf_array",
        dist_binomial_cdf = "Phit\\dist_binomial_cdf", dist_binomial_cdf_array = "Phit\\dist_binomial_cdf_array", dist_binomial_logcdf = "Phit\\dist_binomial_logcdf", dist_binomial_logcdf_array = "Phit\\dist_binomial_logcdf_array",
        dist_binomial_sf = "Phit\\dist_binomial_sf", dist_binomial_sf_array = "Phit\\dist_binomial_sf_array", dist_binomial_logsf = "Phit\\dist_binomial_logsf", dist_binomial_logsf_array = "Phit\\dist_binomial_logsf_array",
        dist_binomial_quantile = "Phit\\dist_binomial_quantile", dist_binomial_quantile_array = "Phit\\dist_binomial_quantile_array"
    ]
);
distribution!(
    register: register_poisson,
    rng: dist_poisson_rng = "Phit\\dist_poisson_rng",
    point: k, counts, density: pmf, logpmf,
    params: (lambda: Real) => dist::Poisson::new(lambda.0),
    functions: [
        dist_poisson_pmf = "Phit\\dist_poisson_pmf", dist_poisson_pmf_array = "Phit\\dist_poisson_pmf_array", dist_poisson_logpmf = "Phit\\dist_poisson_logpmf", dist_poisson_logpmf_array = "Phit\\dist_poisson_logpmf_array",
        dist_poisson_cdf = "Phit\\dist_poisson_cdf", dist_poisson_cdf_array = "Phit\\dist_poisson_cdf_array", dist_poisson_logcdf = "Phit\\dist_poisson_logcdf", dist_poisson_logcdf_array = "Phit\\dist_poisson_logcdf_array",
        dist_poisson_sf = "Phit\\dist_poisson_sf", dist_poisson_sf_array = "Phit\\dist_poisson_sf_array", dist_poisson_logsf = "Phit\\dist_poisson_logsf", dist_poisson_logsf_array = "Phit\\dist_poisson_logsf_array",
        dist_poisson_quantile = "Phit\\dist_poisson_quantile", dist_poisson_quantile_array = "Phit\\dist_poisson_quantile_array"
    ]
);
distribution!(
    register: register_nbinom,
    rng: dist_nbinom_rng = "Phit\\dist_nbinom_rng",
    point: k, counts, density: pmf, logpmf,
    params: (size: Real, prob: Real) => dist::NegativeBinomial::new(size.0, prob.0),
    functions: [
        dist_nbinom_pmf = "Phit\\dist_nbinom_pmf", dist_nbinom_pmf_array = "Phit\\dist_nbinom_pmf_array", dist_nbinom_logpmf = "Phit\\dist_nbinom_logpmf", dist_nbinom_logpmf_array = "Phit\\dist_nbinom_logpmf_array",
        dist_nbinom_cdf = "Phit\\dist_nbinom_cdf", dist_nbinom_cdf_array = "Phit\\dist_nbinom_cdf_array", dist_nbinom_logcdf = "Phit\\dist_nbinom_logcdf", dist_nbinom_logcdf_array = "Phit\\dist_nbinom_logcdf_array",
        dist_nbinom_sf = "Phit\\dist_nbinom_sf", dist_nbinom_sf_array = "Phit\\dist_nbinom_sf_array", dist_nbinom_logsf = "Phit\\dist_nbinom_logsf", dist_nbinom_logsf_array = "Phit\\dist_nbinom_logsf_array",
        dist_nbinom_quantile = "Phit\\dist_nbinom_quantile", dist_nbinom_quantile_array = "Phit\\dist_nbinom_quantile_array"
    ]
);
distribution!(
    register: register_nbinom_mu,
    rng: dist_nbinom_mu_rng = "Phit\\dist_nbinom_mu_rng",
    point: k, counts, density: pmf, logpmf,
    params: (size: Real, mu: Real) => dist::NegativeBinomial::with_mean(size.0, mu.0),
    functions: [
        dist_nbinom_mu_pmf = "Phit\\dist_nbinom_mu_pmf", dist_nbinom_mu_pmf_array = "Phit\\dist_nbinom_mu_pmf_array", dist_nbinom_mu_logpmf = "Phit\\dist_nbinom_mu_logpmf", dist_nbinom_mu_logpmf_array = "Phit\\dist_nbinom_mu_logpmf_array",
        dist_nbinom_mu_cdf = "Phit\\dist_nbinom_mu_cdf", dist_nbinom_mu_cdf_array = "Phit\\dist_nbinom_mu_cdf_array", dist_nbinom_mu_logcdf = "Phit\\dist_nbinom_mu_logcdf", dist_nbinom_mu_logcdf_array = "Phit\\dist_nbinom_mu_logcdf_array",
        dist_nbinom_mu_sf = "Phit\\dist_nbinom_mu_sf", dist_nbinom_mu_sf_array = "Phit\\dist_nbinom_mu_sf_array", dist_nbinom_mu_logsf = "Phit\\dist_nbinom_mu_logsf", dist_nbinom_mu_logsf_array = "Phit\\dist_nbinom_mu_logsf_array",
        dist_nbinom_mu_quantile = "Phit\\dist_nbinom_mu_quantile", dist_nbinom_mu_quantile_array = "Phit\\dist_nbinom_mu_quantile_array"
    ]
);

fn draws(n: i64, seed: i64) -> PhpResult<(usize, u64)> {
    let n =
        usize::try_from(n).map_err(|_| error(ErrorKind::InvalidInput, "n must be nonnegative"))?;
    let seed = u64::try_from(seed)
        .map_err(|_| error(ErrorKind::InvalidInput, "seed must be nonnegative"))?;
    Ok((n, seed))
}

/// A list of ints or floats; keys must be 0..n-1 in order, since draws are
/// positions in it.
fn weights(values: &ZendHashTable, name: &str) -> PhpResult<Vec<f64>> {
    if !values.has_sequential_keys() {
        return Err(error(
            ErrorKind::InvalidInput,
            &format!("{name} must be a list"),
        ));
    }
    values.values().map(element).collect()
}

fn indices(draws: Vec<usize>) -> Vec<i64> {
    draws.into_iter().map(|k| k as i64).collect()
}

/// Uniform(min, max) samples as R's runif(); min == max gives min.
#[php_function]
#[php(name = "Phit\\dist_uniform_rng", defaults(min = 0.0, max = 1.0))]
pub fn dist_uniform_rng(n: i64, seed: i64, min: Real, max: Real) -> PhpResult<Vec<f64>> {
    let (n, seed) = draws(n, seed)?;
    dist::sample_uniform(n, seed, min.0, max.0).map_err(stats_exception)
}

/// n 0-based indices into prob, drawn with those probabilities, as R's
/// sample(x, n, replace = TRUE, prob = prob).
#[php_function]
#[php(name = "Phit\\dist_categorical_rng")]
pub fn dist_categorical_rng(n: i64, seed: i64, prob: &ZendHashTable) -> PhpResult<Vec<i64>> {
    let (n, seed) = draws(n, seed)?;
    let prob = weights(prob, "prob")?;
    dist::sample_categorical(n, seed, &prob)
        .map(indices)
        .map_err(stats_exception)
}

/// One 0-based index per row of prob, drawn with that row's probabilities.
#[php_function]
#[php(name = "Phit\\dist_categorical_rows_rng")]
pub fn dist_categorical_rows_rng(seed: i64, prob: &ZendHashTable) -> PhpResult<Vec<i64>> {
    let (_, seed) = draws(0, seed)?;
    if !prob.has_sequential_keys() {
        return Err(error(
            ErrorKind::InvalidInput,
            "prob must be a list of rows",
        ));
    }
    let rows = prob
        .values()
        .enumerate()
        .map(|(i, row)| {
            let row = row
                .array()
                .ok_or_else(|| error(ErrorKind::InvalidInput, "prob must be a list of rows"))?;
            weights(row, &format!("prob row {i}"))
        })
        .collect::<PhpResult<Vec<_>>>()?;
    dist::sample_categorical_rows(seed, &rows)
        .map(indices)
        .map_err(stats_exception)
}

/// size distinct 0-based indices from 0..n-1 in random order, as R's
/// sample(n, size); size null gives a permutation of all n.
#[php_function]
#[php(name = "Phit\\permutation_rng", defaults(size = None))]
pub fn permutation_rng(n: i64, seed: i64, size: Option<i64>) -> PhpResult<Vec<i64>> {
    let (n, seed) = draws(n, seed)?;
    let size = match size {
        None => n,
        Some(size) => usize::try_from(size)
            .map_err(|_| error(ErrorKind::InvalidInput, "size must be nonnegative"))?,
    };
    dist::sample_permutation(n, seed, size)
        .map(indices)
        .map_err(stats_exception)
}

pub fn register(module: ModuleBuilder) -> ModuleBuilder {
    let module = module
        .function(wrap_function!(dist_uniform_rng))
        .function(wrap_function!(dist_categorical_rng))
        .function(wrap_function!(dist_categorical_rows_rng))
        .function(wrap_function!(permutation_rng));
    let module = register_normal(module);
    let module = register_t(module);
    let module = register_chisquare(module);
    let module = register_f(module);
    let module = register_beta(module);
    let module = register_gamma(module);
    let module = register_exponential(module);
    let module = register_lognormal(module);
    let module = register_logistic(module);
    let module = register_binomial(module);
    let module = register_poisson(module);
    let module = register_nbinom(module);
    register_nbinom_mu(module)
}
