use serde_json::Value;
use stats_core::{ErrorKind, distributions::*};

fn quantile(name: &str, p: &Value) -> Box<dyn Fn(f64) -> f64> {
    let v = |key: &str| p[key].as_f64().unwrap();
    macro_rules! q {
        ($d:expr) => {{
            let d = $d;
            Box::new(move |u| d.quantile(u))
        }};
    }
    match name {
        "uniform" => Box::new(|u| u),
        "normal" => q!(Normal::new(v("mean"), v("sd")).unwrap()),
        "t" => q!(StudentT::new(v("df")).unwrap()),
        "chisquare" => q!(ChiSquare::new(v("df")).unwrap()),
        "f" => q!(FisherF::new(v("df1"), v("df2")).unwrap()),
        "beta" => q!(Beta::new(v("alpha"), v("beta")).unwrap()),
        "gamma" => q!(Gamma::new(v("shape"), v("scale")).unwrap()),
        "exponential" => q!(Exponential::new(v("rate")).unwrap()),
        "lognormal" => q!(LogNormal::new(v("meanlog"), v("sdlog")).unwrap()),
        "logistic" => q!(Logistic::new(v("location"), v("scale")).unwrap()),
        "binomial" => q!(Binomial::new(p["size"].as_u64().unwrap(), v("prob")).unwrap()),
        "poisson" => q!(Poisson::new(v("lambda")).unwrap()),
        "nbinom" => q!(NegativeBinomial::new(v("size"), v("prob")).unwrap()),
        "nbinom_mu" => q!(NegativeBinomial::with_mean(v("size"), v("mu")).unwrap()),
        _ => panic!("unknown family"),
    }
}

#[test]
fn independent_chacha_and_r_quantile_references() {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/rng-python-r.json")).unwrap();
    for case in fixture["cases"].as_array().unwrap() {
        let n = case["n"].as_u64().unwrap() as usize;
        let seed = case["seed"].as_u64().unwrap();
        let family = case["distribution"].as_str().unwrap();
        let q = quantile(family, &case["params"]);
        let result = sample_quantiles(n, seed, &q).unwrap();
        let repeated = sample_quantiles(n, seed, &q).unwrap();
        assert_eq!(result, repeated);
        assert_eq!(sample_quantiles(7, seed, &q).unwrap(), result[..7]);
        for (&actual, expected) in result.iter().zip(case["expected"].as_array().unwrap()) {
            let expected = expected.as_f64().unwrap();
            if family == "uniform"
                || matches!(family, "binomial" | "poisson" | "nbinom" | "nbinom_mu")
            {
                assert_eq!(actual, expected, "{}", case["name"]);
            } else {
                assert!(
                    (actual - expected).abs() <= 1e-15 + 1e-12 * expected.abs(),
                    "{family}: {actual} != {expected}"
                );
            }
        }
    }
}

#[test]
fn sampling_limits_and_no_shared_state() {
    assert!(sample_quantiles(0, 42, |p| p).unwrap().is_empty());
    assert_eq!(
        sample_quantiles(MAX_RNG_DRAWS + 1, 42, |p| p)
            .unwrap_err()
            .kind(),
        ErrorKind::Unsupported
    );
    assert_eq!(
        sample_quantiles(1, 42, |_| f64::INFINITY)
            .unwrap_err()
            .kind(),
        ErrorKind::Nonfinite
    );
    assert_eq!(
        sample_quantiles(1, 42, |_| f64::NAN).unwrap_err().kind(),
        ErrorKind::Nonfinite
    );
    let a = sample_quantiles(128, 42, |p| p).unwrap();
    let b = sample_quantiles(128, 43, |p| p).unwrap();
    assert_ne!(a, b);
    assert_eq!(a, sample_quantiles(128, 42, |p| p).unwrap());
    assert!(a.iter().all(|&p| p > 0. && p < 1.));
}

#[test]
fn generated_normal_and_count_samples_have_expected_moments() {
    let normal = Normal::new(3., 2.).unwrap();
    let poisson = Poisson::new(12.).unwrap();
    let x = sample_quantiles(50000, 173, |p| normal.quantile(p)).unwrap();
    let k = sample_quantiles(50000, 174, |p| poisson.quantile(p)).unwrap();
    let mean = |x: &[f64]| x.iter().sum::<f64>() / x.len() as f64;
    assert!((mean(&x) - 3.).abs() < 0.06);
    assert!((stats_core::descriptive::variance(&x, true).unwrap() - 4.).abs() < 0.15);
    assert!((mean(&k) - 12.).abs() < 0.1);
    assert!((stats_core::descriptive::variance(&k, true).unwrap() - 12.).abs() < 0.4);
    assert!(k.iter().all(|&v| v >= 0. && v.fract() == 0.));
}

/// The fixture's independent Python uniforms for one seed.
fn reference_uniforms(seed: u64) -> Vec<f64> {
    let fixture: Value =
        serde_json::from_str(include_str!("../../../tests/fixtures/rng-python-r.json")).unwrap();
    fixture["cases"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["distribution"] == "uniform" && c["seed"].as_u64() == Some(seed))
        .unwrap()["expected"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_f64().unwrap())
        .collect()
}

#[test]
fn uniform_samples_follow_runif() {
    let u = reference_uniforms(42);
    assert_eq!(sample_uniform(96, 42, 0., 1.).unwrap(), u);
    let scaled = sample_uniform(96, 42, -2., 3.).unwrap();
    for (x, u) in scaled.iter().zip(&u) {
        assert_eq!(*x, -2. + 5. * u);
    }
    assert_eq!(sample_uniform(3, 42, 4., 4.).unwrap(), [4., 4., 4.]);
    assert_eq!(
        sample_uniform(0, 42, 1., 0.).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        sample_uniform(0, 42, f64::NAN, 1.).unwrap_err().kind(),
        ErrorKind::Nonfinite
    );
    assert_eq!(
        sample_uniform(1, 42, -f64::MAX, f64::MAX)
            .unwrap_err()
            .kind(),
        ErrorKind::Nonfinite
    );
}

#[test]
fn categorical_samples_invert_the_cumulative_weights() {
    let u = reference_uniforms(20261012);
    let prob = [0.2, 0., 0.5, 0.3];
    let draws = sample_categorical(96, 20261012, &prob).unwrap();
    for (&k, &u) in draws.iter().zip(&u) {
        let expected = if u < 0.2 {
            0
        } else if u < 0.7 {
            2
        } else {
            3
        };
        assert_eq!(k, expected, "u = {u}");
    }
    // Weights are divided by their sum.
    assert_eq!(
        sample_categorical(96, 20261012, &[2., 0., 5., 3.]).unwrap(),
        draws
    );
    // One row per draw; equal rows reproduce the single-vector form.
    let rows = vec![prob.to_vec(); 96];
    assert_eq!(sample_categorical_rows(20261012, &rows).unwrap(), draws);
    let rows: Vec<Vec<f64>> = (0..96)
        .map(|i| vec![(i % 2) as f64, ((i + 1) % 2) as f64])
        .collect();
    let alternating = sample_categorical_rows(20261012, &rows).unwrap();
    assert!(alternating.iter().enumerate().all(|(i, &k)| k == 1 - i % 2));
    // A zero weight is never drawn, including after rounding at the top.
    assert_eq!(
        sample_categorical(1000, 7, &[0., 1., 0.]).unwrap(),
        vec![1; 1000]
    );
    for bad in [&[][..], &[-0.1, 1.1], &[0., 0.]] {
        assert_eq!(
            sample_categorical(0, 1, bad).unwrap_err().kind(),
            ErrorKind::InvalidInput
        );
    }
    for bad in [&[f64::NAN][..], &[f64::INFINITY], &[f64::MAX, f64::MAX]] {
        assert_eq!(
            sample_categorical(0, 1, bad).unwrap_err().kind(),
            ErrorKind::Nonfinite
        );
    }
    let err = sample_categorical_rows(1, &[vec![1.], vec![]]).unwrap_err();
    assert!(err.to_string().contains("prob row 1"), "{err}");
    assert_eq!(
        sample_categorical(MAX_RNG_DRAWS + 1, 1, &[1.])
            .unwrap_err()
            .kind(),
        ErrorKind::Unsupported
    );
}

#[test]
fn permutations_are_a_partial_fisher_yates_shuffle() {
    let u = reference_uniforms(0);
    let mut expected: Vec<usize> = (0..50).collect();
    for (i, &u) in u.iter().enumerate().take(50) {
        let j = i + (u * (50 - i) as f64) as usize;
        expected.swap(i, j);
    }
    assert_eq!(sample_permutation(50, 0, 50).unwrap(), expected);
    assert_eq!(sample_permutation(50, 0, 7).unwrap(), expected[..7]);
    let mut sorted = sample_permutation(1000, 5, 1000).unwrap();
    sorted.sort_unstable();
    assert_eq!(sorted, (0..1000).collect::<Vec<_>>());
    assert!(sample_permutation(0, 5, 0).unwrap().is_empty());
    assert_eq!(
        sample_permutation(3, 5, 4).unwrap_err().kind(),
        ErrorKind::InvalidInput
    );
    assert_eq!(
        sample_permutation(MAX_RNG_DRAWS + 1, 5, 0)
            .unwrap_err()
            .kind(),
        ErrorKind::Unsupported
    );
}
