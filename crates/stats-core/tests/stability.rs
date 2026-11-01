use serde_json::{Value, json};
use stats_core::*;
fn vector(v: &Value) -> Vec<f64> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_f64().unwrap())
        .collect()
}
fn matrix(v: &Value) -> Vec<Vec<f64>> {
    v.as_array().unwrap().iter().map(vector).collect()
}
fn compare(a: &Value, b: &Value, atol: f64, rtol: f64, label: &str) {
    if let Some(values) = b.as_array() {
        assert_eq!(a.as_array().unwrap().len(), values.len());
        for (a, b) in a.as_array().unwrap().iter().zip(values) {
            compare(a, b, atol, rtol, label)
        }
    } else {
        let a = a.as_f64().unwrap();
        let b = b.as_f64().unwrap();
        assert!(
            a.is_finite() && (a - b).abs() <= atol + rtol * b.abs(),
            "{label}: {a} != {b}"
        );
    }
}
#[test]
fn extreme_but_identified_fits_match_base_r() {
    let f: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/stress-r.json")).unwrap();
    for c in f["cases"].as_array().unwrap() {
        let x = matrix(&c["x"]);
        let y = vector(&c["y"]);
        let w = vector(&c["weights"]);
        let o = vector(&c["offset"]);
        macro_rules! glm{($fit:ident)=>{{let r=$fit(x,y,w,o).unwrap();json!({"coefficients":r.coefficients,"covariance":r.covariance,"std_errors":r.std_errors,"statistics":r.z_statistics,"p_values":r.p_values,"conf_int":r.conf_int,"fitted":r.fitted,"residuals":r.residuals,"log_likelihood":r.log_likelihood,"deviance":r.deviance,"aic":r.aic})}}}
        let a = match c["model"].as_str().unwrap() {
            "ols" => {
                let r = fit_ols_with_data(x, y, w, o).unwrap();
                json!({"coefficients":r.coefficients,"covariance":r.covariance,"std_errors":r.std_errors,"statistics":r.t_statistics,"p_values":r.p_values,"conf_int":r.conf_int,"fitted":r.fitted,"residuals":r.residuals,"log_likelihood":r.log_likelihood})
            }
            "logistic" => glm!(fit_logistic_with_data),
            "probit" => glm!(fit_probit_with_data),
            "poisson" => glm!(fit_poisson_with_data),
            _ => unreachable!(),
        };
        for (key, b) in c["expected"].as_object().unwrap() {
            let response_scale = c["response_scale"].as_f64().unwrap();
            let factor = match key.as_str() {
                "covariance" => response_scale * response_scale,
                "coefficients" | "std_errors" | "conf_int" | "fitted" | "residuals" => {
                    response_scale
                }
                _ => 1.0,
            };
            compare(
                &a[key],
                b,
                c["tolerance"]["absolute"].as_f64().unwrap() * factor,
                c["tolerance"]["relative"].as_f64().unwrap(),
                &format!("{}/{key}", c["name"]),
            );
        }
    }
}
#[test]
fn binomial_count_tolerance_cannot_create_events() {
    macro_rules! check {
        ($fit:ident) => {{
            let x = vec![vec![1.]; 8];
            let y = vec![0., 1., 0., 1., 0., 1., 0., 1.];
            let near = y
                .iter()
                .map(|v| if *v == 0. { 1e-10 } else { 1. - 1e-10 })
                .collect();
            let a = $fit(x.clone(), y, vec![], vec![]).unwrap();
            let b = $fit(x.clone(), near, vec![], vec![]).unwrap();
            assert_eq!(a.coefficients, b.coefficients);
            assert_eq!(a.log_likelihood, b.log_likelihood);
            assert!($fit(x.clone(), vec![1e-10; 8], vec![], vec![]).is_err());
            assert_eq!(
                $fit(x, vec![0.5; 8], vec![1_000_001.; 8], vec![])
                    .unwrap_err()
                    .kind(),
                ErrorKind::Unsupported
            );
        }};
    }
    check!(fit_logistic_with_data);
    check!(fit_probit_with_data);
}
#[test]
fn underflow_is_not_an_ols_perfect_fit() {
    let error = fit_ols(vec![vec![1.]; 4], vec![1e-170, 2e-170, 3e-170, 4e-170]).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::Nonfinite);
    let exact = fit_ols(vec![vec![1.]; 4], vec![0.; 4]).unwrap();
    assert_eq!(exact.rss, 0.);
    assert_eq!(exact.log_likelihood, f64::INFINITY);
}
#[test]
fn dense_mixed_design_limit_is_checked_before_factorization() {
    // Row 2k pairs group k of the first term with group k of the second, and row 2k + 1 with group k + 1, so every
    // random column is connected: one block, which uses the dense summary and its limit.
    let n = 1040;
    let crossed = |groups: Vec<usize>| RandomTerm {
        z: vec![vec![1.]; n],
        groups,
        correlated: false,
    };
    let e = fit_lmm(
        vec![vec![1.]; n],
        (0..n).map(|i| (i % 7) as f64).collect(),
        vec![
            crossed((0..n).map(|i| i / 2).collect()),
            crossed((0..n).map(|i| i.div_ceil(2) % (n / 2)).collect()),
        ],
        Default::default(),
    )
    .unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Unsupported);
}
#[test]
fn an_overflowing_weighted_design_is_rejected_in_blocks_too() {
    // One grouping term splits into blocks; the weighted rows are checked as the dense summary checks them.
    let n = 40;
    let term = RandomTerm {
        z: vec![vec![1.]; n],
        groups: (0..n).map(|i| i / 4).collect(),
        correlated: false,
    };
    // sqrt(1e300) * 1e300 overflows only in the weighted response column.
    let mut y: Vec<f64> = (0..n).map(|i| (i % 7) as f64).collect();
    y[3] = 1e300;
    let e = fit_lmm(
        vec![vec![1.]; n],
        y,
        vec![term],
        LmmOptions {
            weights: vec![1e300; n],
            ..Default::default()
        },
    )
    .unwrap_err();
    assert_eq!(e.kind(), ErrorKind::Nonfinite);
    assert!(
        e.to_string()
            .contains("weighted design exceeds floating-point range"),
        "{e}"
    );
}
#[test]
fn one_grouping_term_has_no_dense_limit() {
    // 513 groups: past the dense limit, but one term splits into a block per group.
    let n = 1026;
    let term = RandomTerm {
        z: vec![vec![1.]; n],
        groups: (0..n).map(|i| i / 2).collect(),
        correlated: false,
    };
    let fit = fit_lmm(
        vec![vec![1.]; n],
        (0..n)
            .map(|i| (i % 7) as f64 + (i / 2 % 5) as f64)
            .collect(),
        vec![term],
        Default::default(),
    )
    .unwrap();
    assert_eq!(fit.terms[0].effects.len(), 513);
}
#[test]
fn error_categories_are_independent_of_message_wording() {
    assert_eq!(
        StatsError::new(ErrorKind::Nonconvergence, "a local optimizer stopped").kind(),
        ErrorKind::Nonconvergence
    );
    assert_eq!(
        fit_ols(vec![vec![1., 1.]; 4], vec![1., 2., 3., 4.])
            .unwrap_err()
            .kind(),
        ErrorKind::RankDeficiency
    );
    assert_eq!(
        fit_ols(vec![vec![1.]; 4], vec![1.]).unwrap_err().kind(),
        ErrorKind::DimensionMismatch
    );
    assert_eq!(
        fit_ols(vec![vec![1.]; 4], vec![1., 2., f64::INFINITY, 4.])
            .unwrap_err()
            .kind(),
        ErrorKind::Nonfinite
    );
}

#[test]
fn overflowing_variation_is_not_an_undefined_r_squared() {
    let x = vec![vec![1., 1.], vec![1., 2.], vec![1., 3.], vec![1., 4.]];
    let y = vec![1e155, 2e155, 3e155, 4e155];
    assert_eq!(
        fit_ols(x.clone(), y.clone()).unwrap_err().kind(),
        ErrorKind::Nonfinite
    );
    assert_eq!(
        fit_ols_with_data(x, y, vec![1., 2., 3., 4.], vec![])
            .unwrap_err()
            .kind(),
        ErrorKind::Nonfinite
    );
}
