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
echo "PHP extreme-input checks passed\n";
