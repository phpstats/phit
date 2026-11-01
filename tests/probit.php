<?php
declare(strict_types=1);
function lcheck(bool $condition, string $message): void {
    if (!$condition) throw new RuntimeException($message);
}
function lcompare(mixed $actual, mixed $expected, string $label): void {
    if (is_array($expected)) {
        lcheck(count($actual) === count($expected), "$label shape");
        foreach ($expected as $i => $value) lcompare($actual[$i], $value, "$label/$i");
    } else {
        lcheck(abs($actual - $expected) <= 1e-9 + 1e-7 * abs($expected), "$label: $actual != $expected");
    }
}
$fixture = json_decode(file_get_contents(__DIR__ . '/fixtures/probit-r.json'), true, flags: JSON_THROW_ON_ERROR);
foreach ($fixture['cases'] as $case) {
    $result = Phit\fit_probit_matrix(x: $case['x'], y: $case['y'], intercept: false);
    foreach ($case['expected'] as $key => $expected) lcompare($result[$key], $expected, $case['name'] . '/' . $key);
    lcheck($result['converged'] === true && $result['iterations'] > 0 && $result['iterations'] <= 100, 'convergence metadata');
}
foreach ([
    [[], []], [[[]], [0]], [[[1], [1,2], [1]], [0,1,0]],
    [[[1], [1], [1]], [0,0.5,1]], [[[1], [1], [1]], [0,true,1]],
    [[[1], [1], [1]], [0,'1',1]], [[[1], [1], [1]], [0,INF,1]],
    [[[1], [1], [1]], [0,0,0]], [[[1,1], [1,1], [1,1]], [0,1,0]],
    [[[1,-2], [1,-1], [1,1], [1,2]], [0,0,1,1]],
    [[[1,-2], [1,-1], [1,0], [1,0], [1,1], [1,2]], [0,0,0,1,1,1]],
    // Invalid original y cannot be hidden by a missing predictor.
    [[[null], [1], [1], [1]], [2,0,1,0]],
    [[[null,INF], [1,1], [1,2], [1,3]], [0,0,1,0]],
    // Deletion removes the only success: no usable two-class sample remains.
    [[[1], [null], [1]], [0,1,0]],
] as [$x,$y]) {
    $thrown = false;
    try { Phit\fit_probit_matrix(x: $x, y: $y, intercept: false); } catch (Throwable $e) { $thrown = true; }
    lcheck($thrown, 'invalid or separated input must throw');
}
$x = $fixture['cases'][0]['x']; $y = $fixture['cases'][0]['y'];
$x[1][1] = null; $y[4] = NAN;
$warnings = [];
set_error_handler(function (int $severity, string $message) use (&$warnings): bool {
    $warnings[] = [$severity,$message]; return true;
});
try { $result = Phit\fit_probit_matrix($x,$y, intercept: false); } finally { restore_error_handler(); }
$completeX = $completeY = [];
foreach ($x as $i => $row) if (!in_array($i,[1,4],true)) { $completeX[] = $row; $completeY[] = $y[$i]; }
$complete = Phit\fit_probit_matrix($completeX,$completeY, intercept: false);
lcompare($result['coefficients'],$complete['coefficients'],'complete-case coefficients');
lcompare($result['fitted'],$complete['fitted'],'complete-case fitted');
lcheck(count($warnings) === 1 && $warnings[0][0] === E_WARNING, 'one native missing-data warning');
lcheck($warnings[0][1]==='Phit\fit_probit_matrix(): dropped 2 of 120 observations with missing values (listwise deletion)', 'warning names function/counts');
lcheck($result['dropped_rows'] === [1,4], 'original dropped row positions');
lcheck($result['retained_rows'] === array_values(array_diff(range(0,119),[1,4])), 'retained row positions');
lcheck($result['n_input'] === 120 && $result['n_observations'] === 118 && $result['df_residual'] === 115, 'retained sample accounting');
foreach (['raise','unknown'] as $policy) {
    $thrown = false;
    try { Phit\fit_probit_matrix($x,$y,$policy, intercept: false); } catch (Throwable $e) { $thrown = true; }
    lcheck($thrown,'missing policy must throw');
}
set_error_handler(function (int $severity, string $message): never { throw new ErrorException($message,0,$severity); });
$thrown = false;
try { Phit\fit_probit_matrix($x,$y, intercept: false); } catch (ErrorException $e) { $thrown = true; } finally { restore_error_handler(); }
lcheck($thrown,'warning promotion');
echo "PHP probit integration tests passed\n";
