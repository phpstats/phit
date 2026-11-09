use serde_json::Value;
use stats_core::ridge::{RidgeOptions, cv_ridge, fit_ridge};
fn fixtures() -> Value {
    serde_json::from_str(include_str!("../../../tests/fixtures/ridge-r.json")).unwrap()
}
fn values(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}
fn close(actual: f64, expected: f64, label: &str) {
    assert!(
        (actual - expected).abs() <= 1e-7 + 2e-6 * expected.abs(),
        "{label}: {actual} != {expected}"
    );
}
#[test]
fn gaussian_ridge_agrees_with_glmnet_simple_designs() {
    let fixture = fixtures();
    for c in fixture["fits"].as_array().unwrap().iter().take(8) {
        let data = &c["data"];
        let y = values(&data["y"]);
        let intercept = !c["formula"].as_str().unwrap().contains("0 +");
        let x: Vec<Vec<f64>> = (0..y.len())
            .map(|i| {
                let mut row = Vec::new();
                if intercept {
                    row.push(1.);
                }
                for name in ["a", "b", "c"] {
                    row.push(data[name][i].as_f64().unwrap());
                }
                row
            })
            .collect();
        let weights = if c["weights"].is_null() {
            vec![1.; y.len()]
        } else {
            values(&data["w"])
        };
        let mut pf = vec![1.; x[0].len()];
        if intercept {
            pf[0] = 0.;
        }
        let mut bounds = vec![[None, None]; pf.len()];
        for (j, name) in ["a", "b", "c"].iter().enumerate() {
            let index = j + usize::from(intercept);
            if let Some(v) = c["penaltyFactor"].get(*name) {
                pf[index] = v.as_f64().unwrap();
            }
            if let Some(v) = c["bounds"].get(*name) {
                bounds[index] = [v[0].as_f64(), v[1].as_f64()];
            }
        }
        let options = RidgeOptions {
            lambda: c["lambda"].as_f64().unwrap(),
            penalty_factor: pf,
            standardize: c["standardize"].as_bool().unwrap(),
            bounds,
            intercept: intercept.then_some(0),
        };
        let fit = fit_ridge(&x, &y, &weights, &options).unwrap();
        for (a, b) in fit
            .coefficients
            .iter()
            .zip(values(&c["expected"]["coefficients"]))
        {
            close(*a, b, c["name"].as_str().unwrap());
        }
        close(fit.df, c["expected"]["df"].as_f64().unwrap(), "df");
        close(fit.sigma, c["expected"]["sigma"].as_f64().unwrap(), "sigma");
        for (a, b) in fit
            .covariance
            .iter()
            .zip(c["expected"]["covariance"].as_array().unwrap())
        {
            for (a, b) in a.iter().zip(b.as_array().unwrap()) {
                match (a, b.as_f64()) {
                    (Some(a), Some(b)) => close(*a, b, "covariance"),
                    (None, None) => (),
                    _ => panic!("nullable covariance"),
                }
            }
        }
    }
}
#[test]
fn unequal_weighted_folds_match_cv_glmnet() {
    let fixtures = fixtures();
    let c = fixtures["cv"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["cv_name"] == "unequal_folds_and_weights")
        .unwrap();
    let d = &c["data"];
    let y = values(&d["y"]);
    let x: Vec<_> = (0..y.len())
        .map(|i| {
            vec![
                1.,
                d["a"][i].as_f64().unwrap(),
                d["b"][i].as_f64().unwrap(),
                d["c"][i].as_f64().unwrap(),
            ]
        })
        .collect();
    let folds: Vec<_> = c["foldid"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_u64().unwrap() as usize)
        .collect();
    let options = RidgeOptions {
        lambda: 0.,
        penalty_factor: vec![0., 1., 1., 1.],
        standardize: true,
        bounds: vec![[None, None]; 4],
        intercept: Some(0),
    };
    let fit = cv_ridge(
        &x,
        &y,
        &values(&d["w"]),
        &options,
        &values(&c["lambda"]),
        &folds,
    )
    .unwrap();
    for (a, b) in fit.cvm.iter().zip(values(&c["expected_cv"]["cvm"])) {
        close(*a, b, "cvm");
    }
    for (a, b) in fit.cvsd.iter().zip(values(&c["expected_cv"]["cvsd"])) {
        close(*a, b, "cvsd");
    }
    assert_eq!(
        fit.index_min,
        c["expected_cv"]["index_min"].as_u64().unwrap() as usize
    );
    assert_eq!(
        fit.index_1se,
        c["expected_cv"]["index_1se"].as_u64().unwrap() as usize
    );
}
