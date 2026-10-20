<?php
declare(strict_types=1);
// Phit\predict versus R's predict() (tests/generate-predict-fixtures.R).
// Fixed-model tolerances follow r-compatibility; GLM standard errors allow the
// documented final-versus-penultimate IRLS weight difference; mixed tolerances
// follow formula-mixed.
function pcheck(bool $condition, string $message): void {
    if (!$condition) throw new RuntimeException($message);
}
$comparisons = 0;
function pcompare(mixed $actual, mixed $expected, string $label, float $abs, float $rel): void {
    global $comparisons;
    if (is_array($expected)) {
        pcheck(is_array($actual) && count($actual) === count($expected), "$label shape");
        foreach ($expected as $i => $value) pcompare($actual[$i] ?? null, $value, "$label/$i", $abs, $rel);
        return;
    }
    if ($expected === null) {
        pcheck($actual === null, "$label: expected a missing prediction, got " . var_export($actual, true));
    } else {
        pcheck(is_float($actual) || is_int($actual), "$label: missing prediction");
        pcheck(abs($actual - $expected) <= $abs + $rel * abs($expected), "$label: $actual != $expected");
    }
    $comparisons++;
}
function fit_case(array $case, array $data): array {
    $args = $case['args'];
    return @match ($case['function']) {
        'lm' => Phit\fit_lm($case['formula'], $data, weights: $args['weights'] ?? null),
        'glm' => Phit\fit_glm($case['formula'], $data, family: $args['family'], link: $args['link']),
        'lmer' => Phit\fit_lmer($case['formula'], $data, reml: $args['reml'] ?? true),
        'glmer' => Phit\fit_glmer($case['formula'], $data),
    };
}
$fixture = json_decode(file_get_contents(__DIR__ . '/fixtures/predict-r.json'), true, flags: JSON_THROW_ON_ERROR);
$cases = 0;
foreach ($fixture['cases'] as $case) {
    $name = $case['name'];
    $fit = fit_case($case, $fixture['datasets'][$case['dataset']]);
    // The R fixture names its options after R's predict(); the PHP parameters are camelCase.
    $options = [];
    foreach ($case['options'] as $option => $value) {
        $options[['se_fit' => 'seFit', 'allow_new_levels' => 'allowNewLevels'][$option] ?? $option] = $value;
    }
    $result = Phit\predict($fit, $fixture['newdata'][$case['dataset']], ...$options);
    foreach ($case['expected'] as $key => $expected) {
        [$abs, $rel] = match (true) {
            $case['function'] === 'lmer' => [1e-5, 1e-5],
            $case['function'] === 'glmer' => [2e-5, 2e-4],
            // R's penultimate-weight vcov; near separation it differs by ~1e-4.
            $case['function'] === 'glm' && $key === 'se_fit' => [1e-6, 1e-3],
            default => [1e-7, 1e-6],
        };
        $actual = $key === 'se_fit_final' ? $result['se_fit'] : ($result[$key] ?? null);
        pcheck($actual !== null || $expected === null, "$name: missing $key");
        pcompare($actual, $expected, "$name/$key", $abs, $rel);
    }
    $missing = array_keys(array_filter($case['expected']['fit'], fn($v) => $v === null));
    pcheck($result['missing_rows'] === $missing, "$name: missing rows");
    $cases++;
}

// Predicting the training data reproduces the fitted values.
$cars = $fixture['datasets']['cars'];
$fit = Phit\fit_lm('mpg ~ poly(hp, 3) + factor(cyl) + wt:am', $cars);
pcompare(Phit\predict($fit, $cars)['fit'], $fit['fitted'], 'lm training data', 1e-12, 1e-12);
$fit = Phit\fit_glm('am ~ wt + hp', $cars, family: 'binomial');
pcompare(Phit\predict($fit, $cars, type: 'response')['fit'], $fit['fitted'], 'glm training data', 1e-12, 1e-10);
$schools = $fixture['datasets']['schools'];
$fit = Phit\fit_lmer('score ~ study_hours + (1 | school)', $schools);
pcompare(Phit\predict($fit, $schools)['fit'], $fit['fitted'], 'lmer training data', 1e-10, 1e-10);
pcompare(Phit\predict($fit, $schools, random: false)['fit'], $fit['fixed_fitted'], 'lmer population', 1e-10, 1e-10);

// Errors match R's: unseen factor levels, unseen groups, type mismatches.
function pthrows(callable $f, string $needle, string $label): void {
    try {
        $f();
    } catch (Exception $e) {
        pcheck(str_contains($e->getMessage(), $needle), "$label: unexpected message " . $e->getMessage());
        return;
    }
    throw new RuntimeException("$label: no exception");
}
$fit = Phit\fit_lm('mpg ~ wt + gear_label', $cars);
pthrows(fn() => Phit\predict($fit, ['wt' => [3.0], 'gear_label' => ['six']]), 'factor gear_label has new level six', 'new level');
pthrows(fn() => Phit\predict($fit, ['wt' => [3.0], 'gear_label' => [4]]), "was a factor", 'numeric for factor');
pthrows(fn() => Phit\predict($fit, ['wt' => ['heavy'], 'gear_label' => ['four']]), "was numeric", 'factor for numeric');
pthrows(fn() => Phit\predict($fit, ['gear_label' => ['four']]), "unknown column 'wt'", 'missing column');
pthrows(fn() => Phit\predict($fit, ['wt' => [3.0], 'gear_label' => ['four']], type: 'probability'), 'type must be', 'type');
$fit = Phit\fit_lmer('score ~ study_hours + (1 | school)', $schools);
pthrows(fn() => Phit\predict($fit, ['study_hours' => [3.0], 'school' => ['nowhere']]), 'new levels detected in newdata: school=nowhere', 'new group');
pthrows(fn() => Phit\predict($fit, ['study_hours' => [3.0], 'school' => ['nowhere']], seFit: true), 'not available for mixed', 'mixed se');
$glm = Phit\fit_glm('am ~ wt', $cars, family: 'binomial');
pthrows(fn() => Phit\predict($glm, $cars, interval: 'confidence'), 'intervals are available for Phit\fit_lm', 'glm interval');
pthrows(fn() => Phit\predict(['formula' => 'y ~ x'], $cars), 'fit must be a result', 'malformed fit');
// A response column is optional; population predictions need no grouping column.
$p = Phit\predict($fit, ['study_hours' => [1.0, 2.0]], random: false);
pcheck(count($p['fit']) === 2 && $p['missing_rows'] === [], 'population prediction without groups');

echo "PHP predict tests passed ($cases R predict() cases; $comparisons comparisons)\n";
