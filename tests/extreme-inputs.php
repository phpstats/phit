<?php
declare(strict_types=1);
// Inputs at numerical extremes: results must stay exact, or fail promptly.
function check(bool $ok, string $message): void {
    if (!$ok) throw new RuntimeException($message);
}
// One prior weight ~1e18 times the others: exact WLS is (-0.4, 16/15).
foreach ([1e12, 1e18, (float) PHP_INT_MAX] as $w) {
    $fit = Phit\fit_ols_matrix([[1], [2], [3], [4], [5], [6]], [1, 2, 2.5, 4, 4.5, 6], weights: [1, 2, 3, 4, 5, $w]);
    check(abs($fit['coefficients'][0] + 0.4) < 1e-9 && abs($fit['coefficients'][1] - 16 / 15) < 1e-9,
        "weighted OLS with weight $w: " . json_encode($fit['coefficients']));
}
// Formula powers beyond the number of variables add no terms and must not loop.
$data = ['y' => [1, 2, 3, 4, 5, 6], 'x' => [1, 3, 2, 5, 4, 6], 'z' => [2, 1, 4, 3, 6, 5]];
$start = hrtime(true);
check(Phit\model_matrix('y ~ (x + z)^1e20', $data)['columns'] === ['(Intercept)', 'x', 'z', 'x:z'], 'huge formula power');
check(hrtime(true) - $start < 1e9, 'huge formula power returns promptly');
// A beta parameter near the largest double once looped forever in the saddle-point deviance term.
// ln f(0.01) = 11.2 ln 0.01 + (1e308 - 1) ln 0.99 - ln B(12.2, 1e308), at 60 digits (mpmath).
$start = hrtime(true);
$logpdf = Phit\dist_beta_logpdf(0.01, 12.2, 1e308);
check(abs($logpdf / -1.0050335853501442e306 - 1.0) <= 1e-13, "beta logpdf with beta = 1e308: $logpdf");
check(Phit\dist_beta_cdf(0.01, 12.2, 1e308) === 1.0 && Phit\dist_beta_sf(0.01, 12.2, 1e308) === 0.0, 'beta cdf and sf with beta = 1e308');
// The quantile and the sampler may report a numerical failure here, but must not hang.
foreach ([fn () => Phit\dist_beta_quantile(0.5, 12.2, PHP_FLOAT_MAX), fn () => Phit\dist_beta_rng(1, 9107391473943085827, 12.205917989158033, 1e308)] as $call) {
    try {
        $call();
    } catch (Exception) {
    }
}
check(hrtime(true) - $start < 1e9, 'beta with a parameter near the largest double returns promptly');
echo "PHP extreme-input checks passed\n";
