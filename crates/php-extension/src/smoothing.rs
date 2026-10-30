//! PHP bindings for kernel density estimation, interpolation and the ECDF.
//!
//! Data arrays follow the descriptive functions: null and NAN are missing,
//! rejected by missing='raise' (the default) or removed by 'drop'; infinite
//! values are errors. Evaluation points (xout, q) may be null, NAN or infinite;
//! a null or NAN point gives NAN, as R gives NA.
use crate::descriptive::{drop_missing, pairs, values};
use crate::distributions::Real;
use crate::stats_exception;
use ext_php_rs::{
    boxed::ZBox, convert::FromZval, convert::StubLiteral, flags::DataType, prelude::*,
    types::ZendHashTable, types::Zval,
};
use stats_core::smoothing::{
    self as smooth, Bandwidth, DensityOptions, Kernel, Method, Rule, Ties,
};
use stats_core::{ErrorKind, StatsError};

fn error(kind: ErrorKind, message: &str) -> PhpException {
    stats_exception(StatsError::new(kind, message))
}

fn invalid(message: &str) -> PhpException {
    error(ErrorKind::InvalidInput, message)
}

fn list(values: Vec<f64>) -> PhpResult<ZBox<ZendHashTable>> {
    let mut out = ZendHashTable::with_capacity(values.len() as u32);
    for v in values {
        out.push(v)?;
    }
    Ok(out)
}

// Evaluation points: numbers, with null read as NAN.
fn points(input: &ZendHashTable, what: &str) -> PhpResult<Vec<f64>> {
    input
        .values()
        .map(|v| {
            if v.is_null() {
                return Ok(f64::NAN);
            }
            v.double()
                .or_else(|| v.long().map(|v| v as f64))
                .ok_or_else(|| invalid(&format!("{what} must be integers, floats, or null")))
        })
        .collect()
}

/// The bw argument: a rule name or a number.
pub enum BwArg {
    Name(String),
    Value(f64),
}

impl FromZval<'_> for BwArg {
    const TYPE: DataType = DataType::Mixed;
    fn from_zval(zval: &Zval) -> Option<Self> {
        zval.str()
            .map(|s| BwArg::Name(s.into()))
            .or_else(|| zval.double().map(BwArg::Value))
            .or_else(|| zval.long().map(|v| BwArg::Value(v as f64)))
    }
}

impl StubLiteral for BwArg {
    fn stub_literal(&self) -> String {
        match self {
            BwArg::Name(name) => format!("'{name}'"),
            BwArg::Value(v) => v.stub_literal(),
        }
    }
}

fn bandwidth(bw: &BwArg) -> PhpResult<Bandwidth> {
    match bw {
        BwArg::Name(name) => match name.to_ascii_lowercase().as_str() {
            "nrd0" => Ok(Bandwidth::Nrd0),
            "nrd" => Ok(Bandwidth::Nrd),
            _ => Err(invalid("bw must be 'nrd0', 'nrd' or a positive number")),
        },
        BwArg::Value(v) => Ok(Bandwidth::Fixed(*v)),
    }
}

/// Kernel density estimate, as R's density(): x and y on an n-point grid,
/// the bandwidth used and the number of observations.
#[php_function]
#[php(
    name = "Phit\\stat_density",
    defaults(
        bw = BwArg::Name("nrd0".into()),
        adjust = 1.0,
        kernel = "\"gaussian\"",
        n = 512,
        cut = 3.0,
        missing = "\"raise\""
    )
)]
#[allow(clippy::too_many_arguments)]
pub fn stat_density(
    x: &ZendHashTable,
    bw: BwArg,
    adjust: Real,
    kernel: &str,
    n: i64,
    from: Option<Real>,
    to: Option<Real>,
    cut: Real,
    weights: Option<&ZendHashTable>,
    missing: &str,
) -> PhpResult<ZBox<ZendHashTable>> {
    let kernel = Kernel::from_name(kernel).ok_or_else(|| {
        invalid(
            "kernel must be 'gaussian', 'epanechnikov', 'rectangular', 'triangular', 'biweight', 'cosine' or 'optcosine'",
        )
    })?;
    if n < 1 {
        return Err(invalid("n must be at least 1"));
    }
    let (x, weights) = match weights {
        None => (values(x, missing)?, None),
        Some(w) => {
            // As R's na.rm: weights that summed to one are renormalized after
            // dropping, so the estimate stays a density.
            let total: f64 = w
                .values()
                .filter_map(|v| v.double().or_else(|| v.long().map(|v| v as f64)))
                .sum();
            let (x, mut w) = pairs(x, w, missing)?;
            if drop_missing(missing)? && (total - 1.0).abs() < 1.5e-8 {
                let kept: f64 = w.iter().sum();
                w.iter_mut().for_each(|v| *v /= kept);
            }
            (x, Some(w))
        }
    };
    let options = DensityOptions {
        bw: bandwidth(&bw)?,
        adjust: adjust.0,
        kernel,
        n: n as usize,
        from: from.map(|v| v.0),
        to: to.map(|v| v.0),
        cut: cut.0,
        weights,
    };
    let d = smooth::density(&x, &options).map_err(stats_exception)?;
    let mut out = ZendHashTable::new();
    out.insert("x", list(d.x)?)?;
    out.insert("y", list(d.y)?)?;
    out.insert("bw", d.bw)?;
    out.insert("n", d.n as i64)?;
    Ok(out)
}

/// R's bw.nrd0: Silverman's rule-of-thumb bandwidth.
#[php_function]
#[php(name = "Phit\\stat_bw_nrd0", defaults(missing = "\"raise\""))]
pub fn stat_bw_nrd0(x: &ZendHashTable, missing: &str) -> PhpResult<f64> {
    smooth::bw_nrd0(&values(x, missing)?).map_err(stats_exception)
}

/// R's bw.nrd: Scott's variation of the rule-of-thumb bandwidth.
#[php_function]
#[php(name = "Phit\\stat_bw_nrd", defaults(missing = "\"raise\""))]
pub fn stat_bw_nrd(x: &ZendHashTable, missing: &str) -> PhpResult<f64> {
    smooth::bw_nrd(&values(x, missing)?).map_err(stats_exception)
}

/// Interpolate (x, y) at xout, as R's approx().
#[php_function]
#[php(
    name = "Phit\\stat_approx",
    defaults(
        method = "\"linear\"",
        rule = 1,
        ties = "\"mean\"",
        f = 0.0,
        missing = "\"raise\""
    )
)]
#[allow(clippy::too_many_arguments)]
pub fn stat_approx(
    x: &ZendHashTable,
    y: &ZendHashTable,
    xout: &ZendHashTable,
    method: &str,
    rule: i64,
    ties: &str,
    f: Real,
    missing: &str,
) -> PhpResult<ZBox<ZendHashTable>> {
    let method = match method {
        "linear" => Method::Linear,
        "constant" => Method::Constant(f.0),
        _ => return Err(invalid("method must be 'linear' or 'constant'")),
    };
    let rule = match rule {
        1 => Rule::Missing,
        2 => Rule::Nearest,
        _ => return Err(invalid("rule must be 1 or 2")),
    };
    let ties = match ties {
        "mean" => Ties::Mean,
        "min" => Ties::Min,
        "max" => Ties::Max,
        _ => return Err(invalid("ties must be 'mean', 'min' or 'max'")),
    };
    let (x, y) = pairs(x, y, missing)?;
    let out = smooth::approx(&x, &y, &points(xout, "xout")?, method, rule, ties)
        .map_err(stats_exception)?;
    list(out)
}

/// The empirical CDF of x at each q: the proportion of x at or below q.
#[php_function]
#[php(name = "Phit\\stat_ecdf", defaults(missing = "\"raise\""))]
pub fn stat_ecdf(
    x: &ZendHashTable,
    q: &ZendHashTable,
    missing: &str,
) -> PhpResult<ZBox<ZendHashTable>> {
    let out = smooth::ecdf(&values(x, missing)?, &points(q, "q")?).map_err(stats_exception)?;
    list(out)
}

pub fn register(module: ModuleBuilder) -> ModuleBuilder {
    module
        .function(wrap_function!(stat_density))
        .function(wrap_function!(stat_bw_nrd0))
        .function(wrap_function!(stat_bw_nrd))
        .function(wrap_function!(stat_approx))
        .function(wrap_function!(stat_ecdf))
}
