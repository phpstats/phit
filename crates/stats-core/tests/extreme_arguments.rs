//! cdflib's incomplete beta/gamma loop forever or panic for some extreme but
//! finite parameters. Each call must return (NaN, reported upstream as 1006)
//! promptly instead of hanging the request or panicking.
use stats_core::distributions::*;
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

type Case = (&'static str, fn() -> f64);

fn returns_promptly(name: &str, evaluate: fn() -> f64) -> f64 {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(evaluate());
    });
    rx.recv_timeout(Duration::from_secs(5))
        .unwrap_or_else(|_| panic!("{name} did not return within 5 s"))
}

#[test]
fn extreme_parameters_fail_fast_instead_of_hanging_or_panicking() {
    let cases: [Case; 7] = [
        ("F cdf, df2 = MAX", || {
            FisherF::new(3.0, f64::MAX).unwrap().cdf(2.0)
        }),
        ("NB(mu) cdf, size = MAX", || {
            NegativeBinomial::with_mean(f64::MAX, 3.0).unwrap().cdf(2.0)
        }),
        ("beta quantile, beta = MAX", || {
            Beta::new(1.5, f64::MAX).unwrap().quantile(1e-10)
        }),
        ("gamma cdf, shape = x = 1e30", || {
            Gamma::new(1e30, 1.0).unwrap().cdf(1e30)
        }),
        ("chi-square cdf, df = x = 2e30", || {
            ChiSquare::new(2e30).unwrap().cdf(2e30)
        }),
        ("poisson cdf, lambda = k = 1e30", || {
            Poisson::new(1e30).unwrap().cdf(1e30)
        }),
        ("beta cdf, alpha = beta = 1e308", || {
            Beta::new(1e308, 1e308).unwrap().cdf(0.5)
        }),
    ];
    for (name, evaluate) in cases {
        assert!(
            returns_promptly(name, evaluate).is_nan(),
            "{name} should report failure"
        );
    }
}

#[test]
fn ordinary_parameters_are_unaffected() {
    let value = returns_promptly("F cdf", || FisherF::new(5.0, 3.0).unwrap().cdf(2.0));
    assert!((value - 0.698_452_637_304_924_2).abs() < 1e-15);
}

#[test]
fn tiny_df_t_quantiles_bracket_quickly_with_unchanged_values() {
    // The additive bracket walk once took milliseconds to seconds here; the
    // bisection over the same points must return the identical double.
    let start = std::time::Instant::now();
    let q = StudentT::new(0.01).unwrap().quantile(0.2);
    assert_eq!(q, -3.124_236_391_667_611_3e38);
    assert_eq!(
        StudentT::new(0.01).unwrap().quantile(0.6),
        246_458_794.805_535_44
    );
    assert!(start.elapsed() < Duration::from_millis(50));
}

#[test]
fn beta_with_a_parameter_near_the_largest_double_returns_the_right_values() {
    // The saddle-point binomial term once looped forever here: its deviance term took x + np beyond the largest
    // double. ln f(x) = (a - 1) ln x + (b - 1) ln(1 - x) - ln B(a, b), at 60 digits (mpmath):
    let logpdf = returns_promptly("beta logpdf, beta = 1e308", || {
        Beta::new(12.2, 1e308).unwrap().logpdf(0.01)
    });
    let want = -1.005_033_585_350_144_2e306;
    assert!(
        ((logpdf - want) / want).abs() <= 1e-13,
        "logpdf {logpdf:e}, want {want:e}"
    );
    // At the median, about 1.1868e-307, the deviance term of the failures against nq ≈ 1e308 takes the series.
    let at_median = returns_promptly("beta logpdf at the median, beta = 1e308", || {
        Beta::new(12.2, 1e308)
            .unwrap()
            .logpdf(1.186_833_452_513_425_2e-307)
    });
    let want = 707.042_693_433_071_7;
    assert!(
        ((at_median - want) / want).abs() <= 1e-13,
        "logpdf at the median {at_median}, want {want}"
    );
    assert_eq!(
        returns_promptly("beta pdf, beta = 1e308", || {
            Beta::new(12.2, 1e308).unwrap().pdf(0.01)
        }),
        0.0
    );
    // Nearly all of the mass lies below 1e-306.
    assert_eq!(
        returns_promptly("beta cdf, beta = 1e308", || {
            Beta::new(12.2, 1e308).unwrap().cdf(0.01)
        }),
        1.0
    );
    assert_eq!(
        returns_promptly("beta sf, beta = 1e308", || {
            Beta::new(12.2, 1e308).unwrap().sf(0.01)
        }),
        0.0
    );
    for (name, evaluate) in [
        (
            "beta quantile, beta = 1e308",
            (|| Beta::new(12.2, 1e308).unwrap().quantile(0.5)) as fn() -> f64,
        ),
        ("beta quantile, beta = MAX", || {
            Beta::new(12.2, f64::MAX).unwrap().quantile(0.5)
        }),
        ("beta logpdf, alpha = 1e308", || {
            Beta::new(1e308, 12.2).unwrap().logpdf(0.99)
        }),
    ] {
        returns_promptly(name, evaluate);
    }
}
