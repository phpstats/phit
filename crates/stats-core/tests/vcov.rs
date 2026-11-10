use serde_json::Value;
use stats_core::{
    formula::{Column, Data, DesignOptions, iv_design, model_design},
    vcov::{Vcov, covariance},
};
#[test]
fn sandwich_covariance_agrees_with_saved_public_api_outputs() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/vcov-r.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let data = &fixture["datasets"][case["dataset"].as_str().unwrap()];
        let frame = Data::new(
            data.as_object()
                .unwrap()
                .iter()
                .map(|(name, values)| {
                    let array = values.as_array().unwrap();
                    let column = if array.iter().any(Value::is_string) {
                        Column::Text(
                            array
                                .iter()
                                .map(|v| v.as_str().map(str::to_owned))
                                .collect(),
                        )
                    } else {
                        Column::Numeric(array.iter().map(Value::as_f64).collect())
                    };
                    (name.clone(), column)
                })
                .collect(),
        )
        .unwrap();
        let weights = case["weights"]
            .as_str()
            .map(|name| {
                data[name]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(Value::as_f64)
                    .collect()
            })
            .unwrap_or_else(|| vec![Some(1.); frame.rows()]);
        let option = &case["vcov"];
        let mut extra = vec![weights];
        if option.get("cluster").is_some() {
            extra.push(
                data["day"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(Value::as_f64)
                    .collect(),
            );
        }
        let options = DesignOptions {
            drop_missing: true,
            drop_unused_levels: true,
            extra,
        };
        let formula = case["formula"].as_str().unwrap();
        let (design, z) = if case["fun"] == "fit_iv" {
            let iv = iv_design(formula, &frame, &options).unwrap();
            (iv.structural, Some(iv.z))
        } else {
            (model_design(formula, &frame, &options).unwrap(), None)
        };
        let method = if let Some(s) = option.as_str() {
            Vcov::Hc(s[2..].parse().unwrap())
        } else if option.get("cluster").is_some() {
            Vcov::Cluster {
                groups: design.extra[1].iter().map(|v| *v as usize).collect(),
                hc1: option["type"] != "HC0",
            }
        } else {
            Vcov::NeweyWest {
                lag: option["newey_west"].as_u64().unwrap() as usize,
                adjust: option["adjust"] == true,
            }
        };
        let residuals: Vec<_> = case["expected"]["residuals"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap())
            .collect();
        let (actual, df) = covariance(
            &design.x,
            z.as_deref(),
            &design.extra[0],
            &residuals,
            &method,
        )
        .unwrap();
        assert_eq!(
            df,
            case["expected"]["inference_df"].as_u64().unwrap() as usize
        );
        for (i, row) in actual.iter().enumerate() {
            for (j, a) in row.iter().enumerate() {
                let b = case["expected"]["covariance"][i][j].as_f64().unwrap();
                assert!(
                    (a - b).abs() <= 1e-9 + 1e-7 * b.abs(),
                    "{} [{i},{j}]: {a} vs {b}",
                    case["name"]
                );
            }
        }
    }
}
