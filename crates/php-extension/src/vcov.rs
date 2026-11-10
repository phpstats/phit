//! Covariance options and row-aligned cluster labels for formula models.
use crate::{input_exception, stats_exception};
use ext_php_rs::{
    prelude::*,
    types::{ZendHashTable, Zval},
};
use stats_core::{
    ErrorKind,
    formula::{Column, Data, ModelDesign},
    vcov::Vcov,
};
use std::collections::HashMap;

pub enum VcovArg {
    Iid,
    Value(Zval),
}
impl ext_php_rs::convert::FromZval<'_> for VcovArg {
    const TYPE: ext_php_rs::flags::DataType = ext_php_rs::flags::DataType::Mixed;
    fn from_zval(v: &Zval) -> Option<Self> {
        Some(if v.str() == Some("iid") {
            Self::Iid
        } else {
            Self::Value(v.shallow_clone())
        })
    }
}
impl ext_php_rs::convert::StubLiteral for VcovArg {
    fn stub_literal(&self) -> String {
        "'iid'".into()
    }
}
fn invalid(s: &str) -> PhpException {
    input_exception(ErrorKind::InvalidInput, s)
}
pub struct Spec {
    pub method: Option<Vcov>,
    pub cluster: Option<Vec<Option<f64>>>,
    pub label: String,
}
fn scalar(v: &Zval) -> PhpResult<Option<String>> {
    if v.is_null() {
        Ok(None)
    } else if let Some(s) = v.str() {
        Ok(Some(format!("s:{s}")))
    } else if let Some(i) = v.long() {
        Ok(Some(format!("n:{i}")))
    } else if let Some(f) = v.double() {
        if !f.is_finite() {
            Ok(None)
        } else if f.fract() == 0. && f >= i64::MIN as f64 && f < i64::MAX as f64 {
            Ok(Some(format!("n:{}", f as i64)))
        } else {
            Ok(Some(format!("n:{f}")))
        }
    } else if let Some(b) = v.bool() {
        Ok(Some(format!("b:{b}")))
    } else {
        Err(invalid("cluster labels must be scalar values or null"))
    }
}
impl Spec {
    pub fn parse(arg: &VcovArg, data: &Data) -> PhpResult<Self> {
        let mut out = Self {
            method: None,
            cluster: None,
            label: "iid".into(),
        };
        let VcovArg::Value(v) = arg else {
            return Ok(out);
        };
        if let Some(s) = v.str() {
            let kind = match s {
                "HC0" => 0,
                "HC1" => 1,
                "HC2" => 2,
                "HC3" => 3,
                _ => return Err(invalid("vcov must be iid, HC0-HC3, cluster or newey_west")),
            };
            out.method = Some(Vcov::Hc(kind));
            out.label = s.into();
            return Ok(out);
        }
        let a = v
            .array()
            .ok_or_else(|| invalid("vcov must be a string or an options array"))?;
        if let Some(cluster) = a.get("cluster") {
            if a.iter()
                .any(|(key, _)| !matches!(key.to_string().as_str(), "cluster" | "type"))
            {
                return Err(invalid("cluster options accept only cluster and type"));
            }
            let hc1 = match a.get("type").and_then(Zval::str) {
                None if a.get("type").is_none() => true,
                Some("HC1") => true,
                Some("HC0") => false,
                _ => return Err(invalid("cluster type must be HC0 or HC1")),
            };
            let labels: Vec<Option<String>> = if let Some(name) = cluster.str() {
                match data
                    .column(name)
                    .ok_or_else(|| invalid("unknown cluster column"))?
                {
                    Column::Numeric(v) => v.iter().map(|x| x.map(|x| format!("n:{x}"))).collect(),
                    Column::Logical(v) => v.iter().map(|x| x.map(|x| format!("b:{x}"))).collect(),
                    Column::Text(v) => v
                        .iter()
                        .map(|x| x.as_ref().map(|x| format!("s:{x}")))
                        .collect(),
                    Column::Factor { codes, .. } => {
                        codes.iter().map(|x| x.map(|x| format!("f:{x}"))).collect()
                    }
                    _ => return Err(invalid("invalid cluster column")),
                }
            } else if let Some(values) = cluster.array() {
                values.values().map(scalar).collect::<PhpResult<_>>()?
            } else {
                return Err(invalid("cluster must be a column name or array of labels"));
            };
            if labels.len() != data.rows() {
                return Err(input_exception(
                    ErrorKind::DimensionMismatch,
                    "cluster labels must align with original rows",
                ));
            }
            let mut ids = HashMap::new();
            let mut groups = Vec::new();
            for label in labels {
                groups.push(label.map(|label| {
                    let next = ids.len();
                    *ids.entry(label).or_insert(next) as f64
                }));
            }
            out.cluster = Some(groups);
            out.method = Some(Vcov::Cluster {
                groups: vec![],
                hc1,
            });
            out.label = format!(
                "cluster ({}, {})",
                if hc1 { "HC1" } else { "HC0" },
                cluster.str().unwrap_or("supplied labels")
            );
        } else if let Some(lag) = a.get("newey_west") {
            if a.iter()
                .any(|(key, _)| !matches!(key.to_string().as_str(), "newey_west" | "adjust"))
            {
                return Err(invalid(
                    "Newey-West options accept only newey_west and adjust",
                ));
            }
            let lag = lag
                .long()
                .filter(|l| *l >= 0)
                .ok_or_else(|| invalid("Newey-West lag must be a nonnegative integer"))?
                as usize;
            let adjust = match a.get("adjust") {
                None => false,
                Some(v) => v.bool().ok_or_else(|| invalid("adjust must be boolean"))?,
            };
            out.method = Some(Vcov::NeweyWest { lag, adjust });
            out.label = format!(
                "Newey-West (lag {lag}, adjust {})",
                if adjust { "true" } else { "false" }
            );
        } else {
            return Err(invalid("vcov options require cluster or newey_west"));
        }
        Ok(out)
    }
    pub fn apply(
        &self,
        fit: &mut stats_core::OlsResult,
        design: &ModelDesign,
        z: Option<&[Vec<f64>]>,
        weights: &[f64],
    ) -> PhpResult<usize> {
        let Some(mut method) = self.method.clone() else {
            return Ok(fit.df_residual);
        };
        if let Vcov::Cluster { groups, .. } = &mut method {
            *groups = design
                .extra
                .get(1)
                .ok_or_else(|| invalid("missing aligned cluster labels"))?
                .iter()
                .map(|x| *x as usize)
                .collect();
        }
        let (covariance, df) =
            stats_core::vcov::covariance(&design.x, z, weights, &fit.residuals, &method)
                .map_err(stats_exception)?;
        stats_core::vcov::replace_inference(fit, covariance, df);
        Ok(df)
    }
    pub fn metadata(&self, out: &mut ZendHashTable, arg: &VcovArg, df: usize) -> PhpResult<()> {
        match arg {
            VcovArg::Iid => out.insert("vcov", "iid")?,
            VcovArg::Value(v) => out.insert("vcov", v.shallow_clone())?,
        };
        out.insert("inference_df", df as i64)?;
        out.insert("vcov_label", self.label.clone())?;
        Ok(())
    }
}
