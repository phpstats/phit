//! R-compatible model formulas and model matrices.
//!
//! `model_design` parses a formula such as `y ~ log(x) + f * z + (1 | g)`,
//! evaluates its variables over a column-oriented data set, applies listwise
//! deletion over the variables the formula uses, and builds R's
//! `model.matrix()` with treatment (unordered) or polynomial (ordered)
//! contrasts, R column names and R term ordering. Random-effect bars are
//! returned as explicit z designs and grouping labels.
mod eval;
mod parse;
mod terms;

use crate::{ErrorKind, StatsError};
pub use eval::PolyCoefs;
use eval::{PolyState, Value, drop_unused, evaluate, orthonormal_poly, sorted_levels};
use parse::Expr;
use terms::{Bar, Terms, build_terms, split_bars};

fn error(message: impl Into<String>) -> StatsError {
    StatsError::new(ErrorKind::InvalidInput, message)
}

/// One data column. Text columns become factors with C-locale sorted levels
/// computed after missing-value deletion, as R's model.matrix converts
/// character columns. Factor levels keep the caller's order; the first level is
/// the reference.
#[derive(Clone, Debug, PartialEq)]
pub enum Column {
    Numeric(Vec<Option<f64>>),
    Logical(Vec<Option<bool>>),
    Text(Vec<Option<String>>),
    Factor {
        levels: Vec<String>,
        codes: Vec<Option<usize>>,
        ordered: bool,
    },
    /// A column that cannot be used in a model; it is an error only when referenced.
    Invalid {
        len: usize,
        message: String,
    },
}

impl Column {
    pub fn len(&self) -> usize {
        match self {
            Column::Numeric(v) => v.len(),
            Column::Logical(v) => v.len(),
            Column::Text(v) => v.len(),
            Column::Factor { codes, .. } => codes.len(),
            Column::Invalid { len, .. } => *len,
        }
    }
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Clone, Debug)]
pub struct Data {
    names: Vec<String>,
    columns: Vec<Column>,
    n: usize,
}

impl Data {
    pub fn new(columns: Vec<(String, Column)>) -> Result<Self, StatsError> {
        let n = columns.first().map_or(0, |(_, c)| c.len());
        let mut names: Vec<String> = Vec::with_capacity(columns.len());
        let mut values = Vec::with_capacity(columns.len());
        for (name, column) in columns {
            if names.contains(&name) {
                return Err(error(format!("duplicate column name '{name}'")));
            }
            if column.len() != n {
                return Err(StatsError::new(
                    ErrorKind::DimensionMismatch,
                    format!(
                        "column '{name}' has {} values; every column needs {n}",
                        column.len()
                    ),
                ));
            }
            if let Column::Factor { levels, codes, .. } = &column {
                if codes.iter().flatten().any(|c| *c >= levels.len()) {
                    return Err(error(format!(
                        "factor column '{name}' has a code outside its levels"
                    )));
                }
                let mut sorted = levels.clone();
                sorted.sort();
                sorted.dedup();
                if sorted.len() != levels.len() {
                    return Err(error(format!(
                        "factor column '{name}' has duplicate levels"
                    )));
                }
            }
            names.push(name);
            values.push(column);
        }
        Ok(Self {
            names,
            columns: values,
            n,
        })
    }
    pub fn rows(&self) -> usize {
        self.n
    }
    pub fn column(&self, name: &str) -> Option<&Column> {
        self.names
            .iter()
            .position(|n| n == name)
            .map(|i| &self.columns[i])
    }
}

#[derive(Clone, Debug, Default)]
pub struct DesignOptions {
    /// false: any missing value in a used variable is an error (missing='raise').
    pub drop_missing: bool,
    /// lm()/glm() drop factor levels absent after deletion; model.matrix() keeps them.
    pub drop_unused_levels: bool,
    /// Additional aligned numeric vectors (weights) that participate in deletion.
    pub extra: Vec<Vec<Option<f64>>>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Response {
    Numeric(Vec<f64>),
    /// Factor/character response: 0-based codes into levels.
    Factor {
        levels: Vec<String>,
        codes: Vec<usize>,
    },
    /// cbind(successes, failures).
    Binomial {
        successes: Vec<f64>,
        failures: Vec<f64>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum GroupLabel {
    Integer(i64),
    Text(String),
}

#[derive(Clone, Debug)]
pub struct RandomDesign {
    /// Bar as written after expansion, e.g. "1 + x | g".
    pub label: String,
    /// Grouping expression, e.g. "g" or "g1:g2".
    pub group: String,
    pub columns: Vec<String>,
    pub correlated: bool,
    /// Rows aligned with the retained observations.
    pub z: Vec<Vec<f64>>,
    pub groups: Vec<GroupLabel>,
}

#[derive(Clone, Debug)]
pub struct ModelDesign {
    pub formula: String,
    pub response_name: Option<String>,
    pub response: Option<Response>,
    /// Row-major design, including the "(Intercept)" column when present.
    pub x: Vec<Vec<f64>>,
    pub columns: Vec<String>,
    /// R's assign attribute: 0 for the intercept, 1-based term index otherwise.
    pub assign: Vec<usize>,
    pub intercept: bool,
    pub terms: Vec<String>,
    pub offset: Option<Vec<f64>>,
    pub random: Vec<RandomDesign>,
    /// The extra vectors from DesignOptions restricted to retained rows.
    pub extra: Vec<Vec<f64>>,
    pub n_input: usize,
    pub retained_rows: Vec<usize>,
    pub dropped_rows: Vec<usize>,
    /// Data-dependent state that prediction on new data must reuse.
    pub predvars: Predvars,
}

/// What R keeps in a fit's xlevels and predvars: factor levels after deletion
/// and orthogonal poly() coefficients, keyed by variable or call text.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Predvars {
    pub levels: Vec<(String, Vec<String>)>,
    pub poly: Vec<(String, PolyCoefs)>,
}

/// Values after deletion: numbers or a coded factor with fixed levels.
enum Coded {
    Num(Vec<f64>),
    Mat(Vec<Vec<f64>>, Vec<String>),
    Factor {
        levels: Vec<String>,
        codes: Vec<usize>,
        ordered: bool,
    },
}

fn subset<T: Clone>(v: &[Option<T>], rows: &[usize]) -> Vec<T> {
    rows.iter()
        .map(|i| v[*i].clone().expect("retained rows are complete"))
        .collect()
}

/// Code a variable against the levels it had when the model was fitted, as
/// predict() does; a level the fit never saw is an error, as in R.
fn code_frozen(
    name: &str,
    value: &Value,
    rows: &[usize],
    levels: Option<&[String]>,
) -> Result<Coded, StatsError> {
    let labels: Vec<String> = match value {
        Value::Logical(v) => subset(v, rows)
            .into_iter()
            .map(|b| if b { "TRUE" } else { "FALSE" }.to_string())
            .collect(),
        Value::Text(v) => subset(v, rows),
        Value::Factor {
            levels: own, codes, ..
        } => rows
            .iter()
            .map(|i| own[codes[*i].expect("retained rows are complete")].clone())
            .collect(),
        _ => {
            if levels.is_some() {
                return Err(error(format!(
                    "variable '{name}' was a factor when the model was fitted"
                )));
            }
            return code(value, rows, false);
        }
    };
    let ordered = matches!(value, Value::Factor { ordered: true, .. });
    let Some(levels) = levels else {
        return Err(error(format!(
            "variable '{name}' was numeric when the model was fitted"
        )));
    };
    let codes = labels
        .iter()
        .map(|label| {
            levels
                .iter()
                .position(|l| l == label)
                .ok_or_else(|| error(format!("factor {name} has new level {label}")))
        })
        .collect::<Result<_, _>>()?;
    Ok(Coded::Factor {
        levels: levels.to_vec(),
        codes,
        ordered,
    })
}

fn code(value: &Value, rows: &[usize], drop_levels: bool) -> Result<Coded, StatsError> {
    Ok(match value {
        Value::Num(v) => Coded::Num(subset(v, rows)),
        Value::Mat(cols, names) => Coded::Mat(
            cols.iter().map(|c| subset(c, rows)).collect(),
            names.clone(),
        ),
        Value::Logical(v) => Coded::Factor {
            levels: vec!["FALSE".into(), "TRUE".into()],
            codes: subset(v, rows).into_iter().map(usize::from).collect(),
            ordered: false,
        },
        Value::Text(v) => {
            let kept: Vec<String> = subset(v, rows);
            let levels = sorted_levels(kept.iter());
            let codes = kept
                .iter()
                .map(|s| levels.binary_search(s).unwrap_or(0))
                .collect();
            Coded::Factor {
                levels,
                codes,
                ordered: false,
            }
        }
        Value::Factor {
            levels,
            codes,
            ordered,
        } => {
            let kept: Vec<Option<usize>> = rows.iter().map(|i| codes[*i]).collect();
            let (levels, kept) = if drop_levels {
                drop_unused(levels.clone(), kept)
            } else {
                (levels.clone(), kept)
            };
            Coded::Factor {
                levels,
                codes: kept.into_iter().flatten().collect(),
                ordered: *ordered,
            }
        }
        Value::Str(_) => return Err(error("a string literal cannot be a model variable")),
    })
}

/// Contrast matrix (levels x columns) and column suffixes.
fn contrasts(
    name: &str,
    levels: &[String],
    ordered: bool,
    dummy: bool,
) -> Result<(Vec<Vec<f64>>, Vec<String>), StatsError> {
    let k = levels.len();
    if dummy {
        let m = (0..k)
            .map(|i| (0..k).map(|j| f64::from(u8::from(i == j))).collect())
            .collect();
        return Ok((m, levels.to_vec()));
    }
    if k < 2 {
        return Err(error(format!(
            "contrasts can be applied only to factors with 2 or more levels ('{name}' has {k} after deletion)"
        )));
    }
    if ordered {
        let scores: Vec<f64> = (1..=k).map(|v| v as f64).collect();
        let basis = orthonormal_poly(&scores, k - 1)?;
        let m = (0..k)
            .map(|i| basis.iter().map(|c| c[i]).collect())
            .collect();
        let names = (1..k)
            .map(|d| match d {
                1 => ".L".to_string(),
                2 => ".Q".to_string(),
                3 => ".C".to_string(),
                _ => format!("^{d}"),
            })
            .collect();
        return Ok((m, names));
    }
    let m = (0..k)
        .map(|i| (1..k).map(|j| f64::from(u8::from(i == j))).collect())
        .collect();
    Ok((m, levels[1..].to_vec()))
}

struct Block {
    columns: Vec<Vec<f64>>,
    names: Vec<String>,
    assign: Vec<usize>,
    labels: Vec<String>,
}

fn is_factor(value: &Coded) -> bool {
    matches!(value, Coded::Factor { levels, .. } if levels.len() > 1)
}

/// R's model.matrix for one terms object over coded (retained) variables.
fn build_block(terms: &Terms, coded: &[Option<Coded>], m: usize) -> Result<Block, StatsError> {
    let mut codes: Vec<Vec<u8>> = (0..terms.terms.len())
        .map(|t| terms.terms[t].iter().map(|v| terms.coding(t, *v)).collect())
        .collect();
    if !terms.intercept {
        // First factor (by term, then variable order) is coded by indicators.
        'outer: for (t, term) in terms.terms.iter().enumerate() {
            for (k, v) in term.iter().enumerate() {
                if coded[*v].as_ref().is_some_and(is_factor) {
                    codes[t][k] = 2;
                    break 'outer;
                }
            }
        }
    }
    let mut block = Block {
        columns: Vec::new(),
        names: Vec::new(),
        assign: Vec::new(),
        labels: terms.terms.iter().map(|t| terms.label(t)).collect(),
    };
    if terms.intercept {
        block.columns.push(vec![1.0; m]);
        block.names.push("(Intercept)".into());
        block.assign.push(0);
    }
    for (t, term) in terms.terms.iter().enumerate() {
        let mut current: Vec<(String, Vec<f64>)> = vec![(String::new(), vec![1.0; m])];
        for (k, v) in term.iter().enumerate() {
            let name = &terms.variables[*v].0;
            let parts: Vec<(String, Vec<f64>)> = match coded[*v].as_ref().expect("evaluated") {
                Coded::Num(values) => vec![(name.clone(), values.clone())],
                Coded::Mat(cols, suffixes) => cols
                    .iter()
                    .zip(suffixes)
                    .map(|(c, s)| (format!("{name}{s}"), c.clone()))
                    .collect(),
                Coded::Factor {
                    levels,
                    codes: factor_codes,
                    ordered,
                } => {
                    let (matrix, suffixes) = contrasts(name, levels, *ordered, codes[t][k] == 2)?;
                    suffixes
                        .iter()
                        .enumerate()
                        .map(|(j, s)| {
                            (
                                format!("{name}{s}"),
                                factor_codes.iter().map(|c| matrix[*c][j]).collect(),
                            )
                        })
                        .collect()
                }
            };
            let mut next = Vec::with_capacity(current.len() * parts.len());
            for (pname, pvalues) in &parts {
                for (cname, cvalues) in &current {
                    let label = if cname.is_empty() {
                        pname.clone()
                    } else {
                        format!("{cname}:{pname}")
                    };
                    next.push((
                        label,
                        cvalues.iter().zip(pvalues).map(|(a, b)| a * b).collect(),
                    ));
                }
            }
            current = next;
        }
        for (name, values) in current {
            block.names.push(name);
            block.columns.push(values);
            block.assign.push(t + 1);
        }
    }
    Ok(block)
}

fn group_label(value: &Value, i: usize) -> String {
    match value {
        Value::Num(v) => parse::format_number(v[i].unwrap_or(f64::NAN)),
        Value::Logical(v) => if v[i] == Some(true) { "TRUE" } else { "FALSE" }.into(),
        Value::Text(v) => v[i].clone().unwrap_or_default(),
        Value::Factor { levels, codes, .. } => {
            codes[i].map(|c| levels[c].clone()).unwrap_or_default()
        }
        _ => String::new(),
    }
}

fn check_finite<'a>(values: impl Iterator<Item = &'a f64>, what: &str) -> Result<(), StatsError> {
    for v in values {
        if !v.is_finite() {
            return Err(StatsError::new(
                ErrorKind::Nonfinite,
                format!("formula produced infinite values in the {what} (for example log(0))"),
            ));
        }
    }
    Ok(())
}

struct BarSpec {
    bar: Bar,
    terms: Terms,
    groups: Vec<Value>,
}

/// Parse a formula, evaluate it over data and build the model design.
pub fn model_design(
    formula: &str,
    data: &Data,
    options: &DesignOptions,
) -> Result<ModelDesign, StatsError> {
    design(formula, data, options, None, false)
}

/// The design for new data under a fitted model, as R's predict() builds it:
/// the response is not needed, factors keep the fitted levels, poly() keeps the
/// fitted basis, and rows with missing predictors are reported as dropped
/// rather than raised. Random-effect terms are evaluated only when requested.
pub fn prediction_design(
    formula: &str,
    data: &Data,
    predvars: &Predvars,
    random: bool,
) -> Result<ModelDesign, StatsError> {
    design(
        formula,
        data,
        &DesignOptions {
            drop_missing: true,
            ..Default::default()
        },
        Some((predvars, random)),
        false,
    )
}

fn design(
    formula: &str,
    data: &Data,
    options: &DesignOptions,
    predict: Option<(&Predvars, bool)>,
    allow_empty: bool,
) -> Result<ModelDesign, StatsError> {
    let frozen = predict.map(|(p, _)| p);
    let mut state = PolyState {
        frozen: frozen.map(|p| p.poly.as_slice()),
        recorded: Vec::new(),
    };
    let levels_of = |name: &str| -> Option<&[String]> {
        frozen.and_then(|p| {
            p.levels
                .iter()
                .find(|(n, _)| n == name)
                .map(|(_, l)| l.as_slice())
        })
    };
    let parsed = parse::parse(formula)?;
    let (response_expr, rhs) = match &parsed {
        Expr::Binary(parse::Op::Tilde, l, r) => (Some((**l).clone()), (**r).clone()),
        Expr::Unary('~', r) => (None, (**r).clone()),
        _ => return Err(error("formula must contain '~', e.g. 'y ~ x'")),
    };
    let mut parts = Vec::new();
    let mut bars = Vec::new();
    split_bars(&rhs, &mut parts, &mut bars, false)?;
    let fixed = build_terms(response_expr.as_ref(), &parts, &data.names)?;
    if predict.is_some_and(|(_, random)| !random) {
        bars.clear();
    }
    let mut specs = Vec::new();
    for bar in bars {
        let mut lhs_parts = Vec::new();
        let mut nested = Vec::new();
        split_bars(&bar.lhs, &mut lhs_parts, &mut nested, false)?;
        if !nested.is_empty() {
            return Err(error("random-effect bars cannot be nested"));
        }
        let terms = build_terms(None, &lhs_parts, &data.names)?;
        if !terms.offsets.is_empty() {
            return Err(error("offset() is not allowed inside a random-effect term"));
        }
        if terms.terms.is_empty() && !terms.intercept {
            return Err(error(format!(
                "random-effect term '{}' has no columns",
                bar.label()
            )));
        }
        let groups = bar
            .groups
            .iter()
            .map(|g| evaluate(g, data, &mut state))
            .collect::<Result<Vec<_>, _>>()?;
        if groups
            .iter()
            .any(|g| matches!(g, Value::Mat(..) | Value::Str(_)))
        {
            return Err(error("grouping variables must be single columns"));
        }
        specs.push(BarSpec { bar, terms, groups });
    }
    let n = data.n;
    if n == 0 {
        return Err(error("data must contain at least one row"));
    }
    if options.extra.iter().any(|e| e.len() != n) {
        return Err(StatsError::new(
            ErrorKind::DimensionMismatch,
            "weights must have one value per original observation",
        ));
    }
    let values: Vec<Value> = fixed
        .variables
        .iter()
        .enumerate()
        .map(|(v, (_, e))| {
            if predict.is_some() && Some(v) == fixed.response {
                // predict() does not need the response; it may be absent.
                Ok(Value::Num(vec![Some(0.0); n]))
            } else {
                evaluate(e, data, &mut state)
            }
        })
        .collect::<Result<_, _>>()?;
    let bar_values: Vec<Vec<Value>> = specs
        .iter()
        .map(|s| {
            s.terms
                .variables
                .iter()
                .map(|(_, e)| evaluate(e, data, &mut state))
                .collect::<Result<Vec<_>, _>>()
        })
        .collect::<Result<_, _>>()?;
    let mut retained = Vec::new();
    let mut dropped = Vec::new();
    for i in 0..n {
        let missing = values.iter().any(|v| v.missing(i))
            || options.extra.iter().any(|e| e[i].is_none())
            || bar_values.iter().flatten().any(|v| v.missing(i))
            || specs.iter().flat_map(|s| &s.groups).any(|g| g.missing(i));
        if missing {
            dropped.push(i);
        } else {
            retained.push(i);
        }
    }
    if !dropped.is_empty() && !options.drop_missing {
        return Err(error("missing values found with missing='raise'"));
    }
    if retained.is_empty() && predict.is_none() {
        return Err(error(
            "no complete observations remain after removing missing values",
        ));
    }
    let m = retained.len();
    let drop_levels = options.drop_unused_levels;
    let coded: Vec<Option<Coded>> = values
        .iter()
        .enumerate()
        .map(|(v, value)| {
            if Some(v) == fixed.response {
                Ok(None)
            } else if predict.is_some() {
                let name = &fixed.variables[v].0;
                code_frozen(name, value, &retained, levels_of(name)).map(Some)
            } else {
                code(value, &retained, drop_levels).map(Some)
            }
        })
        .collect::<Result<_, _>>()?;
    for (k, v) in fixed.offsets.iter().enumerate() {
        if !matches!(coded[*v], Some(Coded::Num(_))) {
            return Err(error(format!("offset {} must be numeric", k + 1)));
        }
    }
    let block = build_block(&fixed, &coded, m)?;
    if block.columns.is_empty() && !allow_empty {
        return Err(error(
            "the formula has no predictor columns; keep the intercept or add a term",
        ));
    }
    let x: Vec<Vec<f64>> = (0..m)
        .map(|i| block.columns.iter().map(|c| c[i]).collect())
        .collect();
    check_finite(x.iter().flatten(), "design matrix")?;
    let offset = if fixed.offsets.is_empty() {
        None
    } else {
        let mut total = vec![0.0; m];
        for v in &fixed.offsets {
            if let Some(Coded::Num(values)) = &coded[*v] {
                for (t, o) in total.iter_mut().zip(values) {
                    *t += o;
                }
            }
        }
        check_finite(total.iter(), "offset")?;
        Some(total)
    };
    let response = match fixed.response {
        _ if predict.is_some() => None,
        None => None,
        Some(r) => Some(match &values[r] {
            Value::Num(v) => Response::Numeric(subset(v, &retained)),
            Value::Logical(v) => Response::Numeric(
                subset(v, &retained)
                    .into_iter()
                    .map(|b| f64::from(u8::from(b)))
                    .collect(),
            ),
            Value::Mat(cols, _) if cols.len() == 2 => Response::Binomial {
                successes: subset(&cols[0], &retained),
                failures: subset(&cols[1], &retained),
            },
            Value::Mat(..) => {
                return Err(error(
                    "a matrix response must be cbind(successes, failures)",
                ));
            }
            other => match code(other, &retained, drop_levels)? {
                Coded::Factor { levels, codes, .. } => Response::Factor { levels, codes },
                _ => return Err(error("unsupported response type")),
            },
        }),
    };
    match &response {
        Some(Response::Numeric(v)) => check_finite(v.iter(), "response")?,
        Some(Response::Binomial {
            successes,
            failures,
        }) => check_finite(successes.iter().chain(failures), "response")?,
        _ => {}
    }
    let mut levels = Vec::new();
    let mut record = |variables: &[(String, Expr)], coded: &[Option<Coded>]| {
        for ((name, _), value) in variables.iter().zip(coded) {
            if let Some(Coded::Factor { levels: l, .. }) = value
                && !levels
                    .iter()
                    .any(|(n, _): &(String, Vec<String>)| n == name)
            {
                levels.push((name.clone(), l.clone()));
            }
        }
    };
    record(&fixed.variables, &coded);
    let mut random = Vec::new();
    for (spec, values) in specs.iter().zip(&bar_values) {
        let coded: Vec<Option<Coded>> = values
            .iter()
            .zip(&spec.terms.variables)
            .map(|(v, (name, _))| {
                if predict.is_some() {
                    code_frozen(name, v, &retained, levels_of(name)).map(Some)
                } else {
                    code(v, &retained, drop_levels).map(Some)
                }
            })
            .collect::<Result<_, _>>()?;
        record(&spec.terms.variables, &coded);
        let block = build_block(&spec.terms, &coded, m)?;
        let z: Vec<Vec<f64>> = (0..m)
            .map(|i| block.columns.iter().map(|c| c[i]).collect())
            .collect();
        check_finite(z.iter().flatten(), "random-effect design")?;
        let single_integer = spec.groups.len() == 1
            && matches!(&spec.groups[0], Value::Num(v) if retained.iter().all(|i| v[*i].is_some_and(|x| x.fract() == 0.0 && x.abs() < 9.0e15)));
        let groups = retained
            .iter()
            .map(|i| {
                if single_integer && let Value::Num(v) = &spec.groups[0] {
                    GroupLabel::Integer(v[*i].unwrap_or(0.0) as i64)
                } else {
                    GroupLabel::Text(
                        spec.groups
                            .iter()
                            .map(|g| group_label(g, *i))
                            .collect::<Vec<_>>()
                            .join(":"),
                    )
                }
            })
            .collect();
        random.push(RandomDesign {
            label: spec.bar.label(),
            group: spec.bar.group_label(),
            columns: block.names,
            correlated: spec.bar.correlated,
            z,
            groups,
        });
    }
    let extra = options
        .extra
        .iter()
        .map(|e| subset(e, &retained))
        .collect::<Vec<_>>();
    check_finite(extra.iter().flatten(), "weights")?;
    Ok(ModelDesign {
        formula: parse::deparse(&parsed),
        response_name: fixed.response.map(|r| fixed.variables[r].0.clone()),
        response,
        x,
        columns: block.names,
        assign: block.assign,
        intercept: fixed.intercept,
        terms: block.labels,
        offset,
        random,
        extra,
        n_input: n,
        retained_rows: retained,
        dropped_rows: dropped,
        predvars: Predvars {
            levels,
            poly: state.recorded,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn data() -> Data {
        let text = |v: &[&str]| Column::Text(v.iter().map(|s| Some(s.to_string())).collect());
        Data::new(vec![
            (
                "y".into(),
                Column::Numeric(vec![
                    Some(1.0),
                    Some(3.0),
                    Some(2.0),
                    Some(5.0),
                    Some(4.0),
                    None,
                ]),
            ),
            (
                "x".into(),
                Column::Numeric(vec![
                    Some(1.0),
                    Some(2.0),
                    Some(3.0),
                    Some(4.0),
                    Some(5.0),
                    Some(6.0),
                ]),
            ),
            ("a".into(), text(&["p", "q", "r", "p", "q", "r"])),
            ("b".into(), text(&["u", "v", "u", "v", "u", "v"])),
        ])
        .unwrap()
    }

    fn design(formula: &str) -> ModelDesign {
        model_design(
            formula,
            &data(),
            &DesignOptions {
                drop_missing: true,
                ..Default::default()
            },
        )
        .unwrap()
    }

    #[test]
    fn interaction_naming_and_order_match_r() {
        assert_eq!(
            design("y ~ a:b").columns,
            [
                "(Intercept)",
                "ap:bu",
                "aq:bu",
                "ar:bu",
                "ap:bv",
                "aq:bv",
                "ar:bv"
            ]
        );
        assert_eq!(
            design("y ~ b:a + a").columns,
            ["(Intercept)", "aq", "ar", "bv:ap", "bv:aq", "bv:ar"]
        );
        assert_eq!(
            design("y ~ a*b - 1").columns,
            ["ap", "aq", "ar", "bv", "aq:bv", "ar:bv"]
        );
        assert_eq!(
            design("y ~ a/x").columns,
            ["(Intercept)", "aq", "ar", "ap:x", "aq:x", "ar:x"]
        );
        let d = design("y ~ poly(x, 2, raw = TRUE) + I(x > 3) + log(x, base = 2)");
        assert_eq!(
            d.columns,
            [
                "(Intercept)",
                "poly(x, 2, raw = TRUE)1",
                "poly(x, 2, raw = TRUE)2",
                "I(x > 3)TRUE",
                "log(x, base = 2)"
            ]
        );
        assert_eq!(d.dropped_rows, [5]);
        assert_eq!(d.assign, [0, 1, 1, 2, 3]);
    }

    #[test]
    fn contr_poly_matches_r() {
        let (m, names) =
            contrasts("o", &["a".into(), "b".into(), "c".into()], true, false).unwrap();
        assert_eq!(names, [".L", ".Q"]);
        assert!((m[0][0] + std::f64::consts::FRAC_1_SQRT_2).abs() < 1e-15);
        assert!((m[1][1] + 0.816_496_580_927_726).abs() < 1e-15);
    }

    #[test]
    fn prediction_reuses_fitted_levels_and_poly_basis() {
        let fitted = design("y ~ poly(x, 2) + a + I(x > 3)");
        assert_eq!(fitted.predvars.levels[0].0, "a");
        assert_eq!(fitted.predvars.poly[0].0, "poly(x, 2)");
        let again = prediction_design(
            "y ~ poly(x, 2) + a + I(x > 3)",
            &data(),
            &fitted.predvars,
            false,
        )
        .unwrap();
        // The training rows rebuild the training design; the row whose only
        // missing value is the response is predicted too.
        assert_eq!(again.retained_rows, [0, 1, 2, 3, 4, 5]);
        for (a, b) in again.x.iter().flatten().zip(fitted.x.iter().flatten()) {
            assert!((a - b).abs() < 1e-14, "{a} != {b}");
        }
        // One new row: level q only, x outside the fitted range, no response column.
        let new = Data::new(vec![
            ("x".into(), Column::Numeric(vec![Some(9.0)])),
            ("a".into(), Column::Text(vec![Some("q".into())])),
        ])
        .unwrap();
        let row = prediction_design(
            "y ~ poly(x, 2) + a + I(x > 3)",
            &new,
            &fitted.predvars,
            false,
        )
        .unwrap();
        assert_eq!(row.columns, fitted.columns);
        assert_eq!(row.x[0][3..], [1.0, 0.0, 1.0]);
        let unseen = Data::new(vec![
            ("x".into(), Column::Numeric(vec![Some(2.0)])),
            ("a".into(), Column::Text(vec![Some("z".into())])),
        ])
        .unwrap();
        let message = prediction_design("y ~ a + x", &unseen, &design("y ~ a + x").predvars, false)
            .unwrap_err()
            .to_string();
        assert!(message.contains("factor a has new level z"), "{message}");
    }

    #[test]
    fn bars_are_split() {
        let d = design("y ~ x + (1 + x | a) + (1 | a:b)");
        assert_eq!(d.columns, ["(Intercept)", "x"]);
        assert_eq!(d.random.len(), 2);
        assert_eq!(d.random[0].columns, ["(Intercept)", "x"]);
        assert_eq!(d.random[1].group, "a:b");
        assert_eq!(d.random[1].groups[0], GroupLabel::Text("p:u".into()));
    }
}

mod iv;
pub use iv::{IvDesign, iv_design, split_iv_formula};
