//! IV formula preparation; bars split only outside parentheses and quoted names.
use super::*;

pub struct IvDesign {
    pub structural: ModelDesign,
    pub endogenous: Vec<usize>,
    pub instruments: Vec<String>,
    pub instrument_columns: Vec<String>,
    pub z: Vec<Vec<f64>>,
}

/// Find exactly one top-level separator, respecting quoted names and strings.
pub fn split_iv_formula(formula: &str) -> Result<(&str, &str, &str), StatsError> {
    let mut depth = 0i32;
    let mut quote = None;
    let mut escaped = false;
    let mut bar = None;
    let mut tilde = None;
    for (i, c) in formula.char_indices() {
        if let Some(q) = quote {
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == q {
                quote = None;
            }
            continue;
        }
        match c {
            '`' | '\'' | '"' => quote = Some(c),
            '(' => depth += 1,
            ')' => depth -= 1,
            '~' if depth == 0 && bar.is_some() && tilde.replace(i).is_some() => {
                return Err(error("IV formula needs one ~ after |"));
            }
            '|' if depth == 0 && bar.replace(i).is_some() => {
                return Err(error("IV formula requires exactly one top-level |"));
            }
            _ => (),
        }
        if depth < 0 {
            return Err(error("unbalanced IV formula parentheses"));
        }
    }
    if depth != 0 || quote.is_some() {
        return Err(error("unbalanced IV formula"));
    }
    let bar = bar.ok_or_else(|| error("IV formula must be 'y ~ exog | endog ~ instruments'"))?;
    let tilde = tilde.ok_or_else(|| error("IV formula needs endog ~ instruments after |"))?;
    let left = &formula[..bar];
    let endog = &formula[bar + 1..tilde];
    let instruments = &formula[tilde + 1..];
    if endog.trim().is_empty() || instruments.trim().is_empty() {
        return Err(error(
            "IV formula needs endogenous regressors and instruments",
        ));
    }
    Ok((left.trim(), endog.trim(), instruments.trim()))
}

pub fn iv_design(
    formula: &str,
    data: &Data,
    options: &DesignOptions,
) -> Result<IvDesign, StatsError> {
    let (left, endog, instruments) = split_iv_formula(formula)?;
    let structural_formula = format!("{left} + {endog}");
    // Preliminary passes identify the union of missing rows. Rebuild all designs
    // on that union so factor levels and data-dependent transforms agree.
    let instrument_formula = format!("~ {instruments}");
    let preliminary = DesignOptions {
        drop_missing: true,
        ..options.clone()
    };
    let a = model_design(&structural_formula, data, &preliminary)?;
    let b = model_design(&instrument_formula, data, &preliminary)?;
    for d in [&a, &b] {
        if !d.random.is_empty() {
            return Err(StatsError::new(
                ErrorKind::Unsupported,
                "fit_iv does not support random-effect bars",
            ));
        }
    }
    let mut shared = options.clone();
    shared.extra.push(
        (0..data.rows())
            .map(|i| {
                if a.retained_rows.contains(&i) && b.retained_rows.contains(&i) {
                    Some(1.)
                } else {
                    None
                }
            })
            .collect(),
    );
    let structural = model_design(&structural_formula, data, &shared)?;
    let exog = design(left, data, &shared, None, true)?;
    // Use the structural intercept convention for instrument factor coding.
    let instrument_formula = format!(
        "{left} + ({instruments}) + {}",
        if exog.intercept { "1" } else { "0" }
    );
    let excluded = model_design(&instrument_formula, data, &shared)?;
    for d in [&structural, &exog, &excluded] {
        if !d.random.is_empty() {
            return Err(StatsError::new(
                ErrorKind::Unsupported,
                "fit_iv does not support random-effect bars",
            ));
        }
        if d.offset.is_some() {
            return Err(StatsError::new(
                ErrorKind::Unsupported,
                "fit_iv does not support offsets",
            ));
        }
    }
    if structural.intercept != exog.intercept {
        return Err(error(
            "specify intercept removal in the exogenous formula part",
        ));
    }
    if exog
        .columns
        .iter()
        .any(|c| !structural.columns.contains(c) || !excluded.columns.contains(c))
    {
        return Err(error(
            "endogenous and instrument parts must retain every exogenous column",
        ));
    }
    let endogenous: Vec<_> = structural
        .columns
        .iter()
        .enumerate()
        .filter_map(|(j, c)| (!exog.columns.contains(c)).then_some(j))
        .collect();
    if endogenous.is_empty() {
        return Err(error(
            "IV formula must add at least one endogenous design column",
        ));
    }
    let excluded_indices: Vec<_> = excluded
        .columns
        .iter()
        .enumerate()
        .filter_map(|(j, c)| (c != "(Intercept)" && !exog.columns.contains(c)).then_some(j))
        .collect();
    let instruments: Vec<_> = excluded_indices
        .iter()
        .map(|j| excluded.columns[*j].clone())
        .collect();
    let instrument_columns = excluded.columns.clone();
    let z = excluded.x;
    Ok(IvDesign {
        structural,
        endogenous,
        instruments,
        instrument_columns,
        z,
    })
}
