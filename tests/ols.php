<?php
declare(strict_types=1);
function check(bool $condition, string $message): void {
    if (!$condition) throw new RuntimeException($message);
}
function compare(mixed $actual, mixed $expected, string $label): void {
    if (is_array($expected)) {
        check(count($actual) === count($expected), "$label shape");
        foreach ($expected as $i => $value) compare($actual[$i], $value, "$label/$i");
    } else {
        check(abs($actual - $expected) <= 1e-9 + 1e-7 * abs($expected), "$label: $actual != $expected");
    }
}
$fixture = json_decode(file_get_contents(__DIR__ . '/fixtures/ols-r.json'), true, flags: JSON_THROW_ON_ERROR);
foreach ($fixture['cases'] as $case) {
    $result = Phit\fit_ols(x: $case['x'], y: $case['y'], intercept: false);
    foreach ($case['expected'] as $key => $expected) {
        if (in_array($case['name'], ['perfect', 'zero'], true) && in_array($key, ['t_statistics', 'p_values', 'log_likelihood', 'r_squared', 'adj_r_squared', 'conf_int'], true)) continue;
        compare($result[$key], $expected, $case['name'] . '/' . $key);
    }
    if ($case['name'] === 'zero') {
        check(is_nan($result['r_squared']), 'zero R squared');
        check(is_nan($result['t_statistics'][0]), 'zero t statistic');
        check($result['log_likelihood'] === INF, 'zero likelihood');
        check($result['conf_int'] === [[0.0, 0.0], [0.0, 0.0]], 'zero intervals');
    }
}
foreach ([
    [[], []], [[[]], [1]], [[[1], [1, 2]], [1, 2]],
    [[[1], [2]], [1]], [[[1, 2], [1, 3]], [1, 2]],
    [[[1, 1], [1, 1], [1, 1]], [1, 2, 3]],
    [[[NAN], [1]], [1, 2]], [[[1], [2]], [INF, 2]],
    [[[true], [2]], [1, 2]], [[['1'], [2]], [1, 2]],
    [[1, 2], [1, 2]], [[[1], [2]], [null, 2]],
] as [$x, $y]) {
    $thrown = false;
    try { Phit\fit_ols($x, $y, intercept: false); } catch (Throwable $e) { $thrown = true; }
    check($thrown, 'invalid input must throw');
}
$thrown = false;
try { Phit\fit_ols('bad', [], intercept: false); } catch (Throwable $e) { $thrown = true; }
check($thrown, 'non-array input must throw');
// Default listwise deletion is visible and preserves original row positions.
$warnings = [];
set_error_handler(function (int $severity, string $message) use (&$warnings): bool {
    $warnings[] = [$severity, $message];
    return true;
});
$x = [[1, 1], [1, null], [1, 2], [1, 3], [1, 4], [1, NAN], [1, 5]];
$y = [2.1, 99, 4.2, null, 8.1, 99, 10.2];
try { $dropped = Phit\fit_ols($x, $y, intercept: false); } finally { restore_error_handler(); }
$complete = Phit\fit_ols([[1, 1], [1, 2], [1, 4], [1, 5]], [2.1, 4.2, 8.1, 10.2], intercept: false);
compare($dropped['coefficients'], $complete['coefficients'], 'complete-case coefficients');
compare($dropped['fitted'], $complete['fitted'], 'complete-case fitted');
check($dropped['dropped_rows'] === [1, 3, 5], 'dropped row positions');
check($dropped['retained_rows'] === [0, 2, 4, 6], 'retained row positions');
check($dropped['n_input'] === 7 && $dropped['n_observations'] === 4 && $dropped['n_dropped'] === 3, 'sample accounting');
check($dropped['df_residual'] === 2, 'degrees of freedom after deletion');
check(count($warnings) === 1 && $warnings[0][0] === E_WARNING, 'exactly one native warning');
check(str_contains($warnings[0][1], '3 of 7'), 'warning includes sample counts');
check($complete['n_dropped'] === 0 && $complete['dropped_rows'] === [], 'complete data metadata');
check($complete['retained_rows'] === [0, 1, 2, 3], 'complete data positions');

// Validation cannot be hidden by a missing value elsewhere in the same row.
foreach ([
    [$x, $y, 'raise'], [$x, $y, 'unknown'],
    [[[1, null], [1, 2]], [null, null], 'drop'],
    [[[1, null], [1, 2], [1, 3]], [1, 2, 3], 'drop'],
    [[[1, null], [1, 1], [1, 1], [1, 1]], [1, 2, 3, 4], 'drop'],
    [[[null, 'bad'], [1, 2], [1, 3], [1, 4]], [1, 2, 3, 4], 'drop'],
    [[[null, INF], [1, 2], [1, 3], [1, 4]], [1, 2, 3, 4], 'drop'],
    [[[1, null], [1], [1, 3], [1, 4]], [1, 2, 3, 4], 'drop'],
] as [$badX, $badY, $policy]) {
    $thrown = false;
    try { Phit\fit_ols(x: $badX, y: $badY, missing: $policy, intercept: false); } catch (Throwable $e) { $thrown = true; }
    check($thrown, 'missing-data validation must throw');
}
// A PHP application can promote the warning to an exception.
set_error_handler(function (int $severity, string $message): never { throw new ErrorException($message, 0, $severity); });
$thrown = false;
try { Phit\fit_ols($x, $y, intercept: false); } catch (ErrorException $e) { $thrown = true; } finally { restore_error_handler(); }
check($thrown, 'warning-to-exception handler is respected');
echo "PHP OLS integration tests passed\n";
