<?php
declare(strict_types=1);

function intercept_check(bool $condition, string $label): void {
    if (!$condition) throw new RuntimeException($label);
}
function intercept_compare(mixed $actual, mixed $expected, string $label): void {
    if (is_array($expected)) {
        intercept_check(array_keys($actual) === array_keys($expected), "$label keys");
        foreach ($expected as $key => $value) intercept_compare($actual[$key], $value, "$label/$key");
    } elseif (is_float($expected)) {
        intercept_check(is_nan($expected) ? is_nan($actual) : $actual === $expected, "$label value");
    } else {
        intercept_check($actual === $expected, "$label value");
    }
}
// Stored reference matrices include ones. Removing those ones and using the new
// default must produce exactly the same numerical inputs/results in every family.
foreach ([
    'Phit\fit_ols' => 'ols-r', 'Phit\fit_logistic' => 'logistic-r',
    'Phit\fit_probit' => 'probit-r', 'Phit\fit_poisson' => 'poisson-r',
    'Phit\fit_mixed_intercept' => 'lmm-r', 'Phit\fit_mixed' => 'lmm-expanded-r',
    'Phit\fit_logistic_mixed' => 'logistic-mixed-r',
] as $function => $fixture) {
    $case = json_decode(file_get_contents(__DIR__."/fixtures/$fixture.json"), true, flags: JSON_THROW_ON_ERROR)['cases'][0];
    $args = ['x' => $case['x'], 'y' => $case['y']];
    foreach (['groups' => 'groups', 'weights' => 'weights', 'offset' => 'offset',
        'reml' => 'reml', 'quadrature_points' => 'quadraturePoints'] as $key => $argument) {
        if (array_key_exists($key, $case)) $args[$argument] = $case[$key];
    }
    if (isset($case['terms'])) $args['random'] = $case['terms'];
    $literal = $function(...$args, intercept: false);
    $args['x'] = array_map(fn($row) => array_slice($row, 1), $case['x']);
    $copy = $args;
    intercept_compare($function(...$args), $literal, "$function default intercept");
    intercept_check($args === $copy, "$function must not mutate inputs");
}
// No predictor columns is a supported intercept-only model, not an empty sample.
$x = array_fill(0, 6, []);
$r = Phit\fit_ols($x, [2, 3, 7, 4, 8, 6]);
intercept_check(count($r['coefficients']) === 1 && abs($r['coefficients'][0] - 5) < 1e-12 && $r['df_residual'] === 5, 'intercept-only OLS');
foreach (['Phit\fit_logistic', 'Phit\fit_probit', 'Phit\fit_poisson'] as $function) {
    $y = $function === 'Phit\fit_poisson' ? [1, 3, 2, 5, 4, 2] : [0, 1, 0, 0, 1, 0];
    intercept_compare($function($x, $y), $function(array_fill(0, 6, [1]), $y, intercept: false), "$function intercept-only");
}
// The default always prepends a column; it does not guess which supplied columns
// represent an intercept. Supplying ones twice must retain rank-deficiency errors.
try {
    Phit\fit_ols(array_fill(0, 6, [1]), [2, 3, 7, 4, 8, 6]);
    throw new RuntimeException('duplicate intercept accepted');
} catch (Exception $e) {
    intercept_check($e->getCode() === 1004, 'duplicate intercept rank error');
}
foreach ([[[], []], [[[1], []], [1, 2]], [$x, [1, 2, 3, 4, 5, 6], false]] as $invalid) {
    try {
        Phit\fit_ols($invalid[0], $invalid[1], intercept: $invalid[2] ?? true);
        throw new RuntimeException('invalid design accepted');
    } catch (Exception $e) {
        intercept_check(in_array($e->getCode(), [1001, 1002], true), 'invalid design category');
    }
}
// A genuinely no-intercept design still uses uncentered R-squared.
$r = Phit\fit_ols([[2], [3], [5], [7], [11]], [5, 6, 9, 15, 20], intercept: false);
intercept_check(count($r['coefficients']) === 1 && $r['df_residual'] === 4, 'no-intercept coefficient count');
$tss = array_sum(array_map(fn($y) => $y * $y, [5, 6, 9, 15, 20]));
intercept_check(abs($r['r_squared'] - (1 - $r['rss'] / $tss)) < 1e-12, 'uncentered R-squared');

// Independent closed-form check: offset-only Poisson plus an estimated intercept.
// mu_i = exposure_i * rate, rate = sum(w_i*y_i) / sum(w_i*exposure_i).
$exposure = [1, 2, 4, 8, 16, 3, 6, 12];
$counts = [1, 4, 3, 12, 23, 2, 11, 14];
$offset = array_map(fn($e) => log($e), $exposure);
foreach ([array_fill(0, 8, 1), [1, 2, 1, 3, 2, 1, 2, 1]] as $weights) {
    $events = $time = 0;
    foreach ($counts as $i => $count) {
        $events += $weights[$i] * $count;
        $time += $weights[$i] * $exposure[$i];
    }
    $rate = $events / $time;
    $r = Phit\fit_poisson(array_fill(0, 8, []), $counts, weights: $weights, offset: $offset);
    intercept_check(abs($r['coefficients'][0] - log($rate)) < 1e-10, 'Poisson exposure MLE');
    intercept_check(abs($r['std_errors'][0] - 1 / sqrt($events)) < 1e-10, 'Poisson exposure SE');
    $ll = 0;
    foreach ($counts as $i => $count) {
        $mu = $rate * $exposure[$i];
        $logFactorial = array_sum(array_map(fn($j) => log($j), range(1, $count)));
        $ll += $weights[$i] * ($count * log($mu) - $mu - $logFactorial);
        intercept_check(abs($r['fitted'][$i] - $mu) < 1e-9, 'Poisson expected count');
    }
    intercept_check(abs($r['log_likelihood'] - $ll) < 1e-9, 'Poisson exposure likelihood');
    $scaled = Phit\fit_poisson(array_fill(0, 8, []), $counts, weights: $weights, offset: array_map(fn($o) => $o + log(2), $offset));
    intercept_check(abs($scaled['coefficients'][0] - $r['coefficients'][0] + log(2)) < 1e-10, 'exposure units shift intercept');
    foreach ($r['fitted'] as $i => $mu) intercept_check(abs($scaled['fitted'][$i] - $mu) < 1e-9, 'exposure units preserve counts');
}
// Default-intercept Poisson agrees with the existing independently generated R
// weighted/offset fixture, including covariance, tests, likelihood and deviance.
$cases = json_decode(file_get_contents(__DIR__.'/fixtures/r-compatibility.json'), true, flags: JSON_THROW_ON_ERROR)['cases'];
foreach ($cases as $case) {
    if ($case['model'] !== 'poisson' || $case['x'][0][0] !== 1) continue;
    $r = Phit\fit_poisson(array_map(fn($row) => array_slice($row, 1), $case['x']), $case['y'], weights: $case['weights'], offset: $case['offset']);
    $compare = function($a, $b) use (&$compare): void {
        if (is_array($b)) {
            intercept_check(count($a) === count($b), 'R Poisson shape');
            foreach ($b as $key => $value) $compare($a[$key], $value);
        } else intercept_check(abs($a - $b) <= 1e-7 + 1e-6 * abs($b), 'R Poisson numerical agreement');
    };
    foreach ($case['expected'] as $key => $value) $compare($r[$key], $value);
}
$missingOffset = $offset; $missingOffset[2] = null;
$warnings = [];
set_error_handler(function($severity, $message) use (&$warnings) { $warnings[] = $message; return true; });
try { $r = Phit\fit_poisson(array_fill(0, 8, []), $counts, offset: $missingOffset); }
finally { restore_error_handler(); }
intercept_check($r['dropped_rows'] === [2] && $r['df_residual'] === 6 && count($warnings) === 1, 'offset deletion/default intercept accounting');
try {
    Phit\fit_poisson(array_fill(0, 8, []), $counts, missing: 'raise', offset: $missingOffset);
    throw new RuntimeException('missing offset accepted');
} catch (Exception $e) { intercept_check($e->getCode() === 1001, 'strict missing offset'); }
echo "PHP automatic intercepts and Poisson exposure offsets passed\n";
