<?php
declare(strict_types=1);
// Phit\fit_lmer/Phit\fit_glmer formulas versus lme4 (tests/generate-formula-lme4-fixtures.R).
// Tolerances follow lmm-expanded-r (lmer) and logistic-mixed-r (glmer).
function mcheck(bool $condition, string $message): void {
    if (!$condition) throw new RuntimeException($message);
}
$comparisons = 0;
function mcompare(mixed $actual, mixed $expected, string $label, float $abs, float $rel): void {
    global $comparisons;
    if (is_array($expected)) {
        mcheck(is_array($actual) && count($actual) === count($expected), "$label shape");
        foreach ($expected as $i => $value) mcompare($actual[$i] ?? null, $value, "$label/$i", $abs, $rel);
        return;
    }
    mcheck(abs($actual - $expected) <= $abs + $rel * abs($expected), "$label: $actual != $expected");
    $comparisons++;
}
// lme4 ranef(condVar = TRUE) postVar, keyed by lme4 level, against a term's per-group
// blocks. $blocks maps a group label to its k x k block; (x || g) records only the
// diagonal because lme4 splits it into separate one-column terms.
function cond_var_compare(array $blocks, array $labels, array $reference, string $label, float $abs, float $rel): void {
    $index = array_flip(array_map('strval', $labels));
    mcheck(count($index) === count($reference['levels']), "$label levels");
    foreach ($reference['levels'] as $i => $level) {
        $block = $blocks[$index[(string)$level] ?? -1] ?? null;
        mcheck(is_array($block), "$label/$level missing");
        if (isset($reference['cond_var'])) {
            mcompare($block, $reference['cond_var'][$i], "$label/$level", $abs, $rel);
        } else {
            mcompare(array_map(fn($j) => $block[$j][$j], array_keys($block)), $reference['cond_var_diagonal'][$i], "$label/$level", $abs, $rel);
        }
    }
}
$fixture = json_decode(file_get_contents(__DIR__ . '/fixtures/formula-lme4-r.json'), true, flags: JSON_THROW_ON_ERROR);
$fits = ['lmer' => 0, 'glmer' => 0];
foreach ($fixture['cases'] as $case) {
    $data = $fixture['datasets'][$case['dataset']];
    $name = $case['name'];
    $result = @($case['function'] === 'lmer'
        ? Phit\fit_lmer($case['formula'], $data, reml: $case['reml'])
        : Phit\fit_glmer($case['formula'], $data, quadraturePoints: $case['quadrature_points']));
    $expected = $case['expected'];
    mcheck($result['columns'] === $expected['columns'], "$name: columns");
    mcheck($result['n_observations'] === $expected['n_observations'], "$name: observations");
    mcheck(array_column($result['random'], 'group') === array_column($case['bars'], 'group'), "$name: random groups");
    mcheck(array_map(fn($t) => (array)$t['columns'], $result['random']) === array_map(fn($b) => (array)$b['columns'], $case['bars']), "$name: random columns");
    foreach ($expected as $key => $value) {
        if (in_array($key, ['columns', 'n_observations'], true)) continue;
        if ($key === 'random_cond_var') {
            [$abs, $rel] = $case['function'] === 'lmer' ? [1e-5, 1e-5] : [2e-5, 2e-4];
            foreach ($value as $t => $reference) {
                [$blocks, $labels] = $case['function'] === 'lmer'
                    ? [$result['random'][$t]['effects_cond_var'], $result['random'][$t]['group_labels']]
                    : [array_map(fn($v) => [[$v]], $result['random_effects_cond_var']), $result['group_labels']];
                cond_var_compare($blocks, $labels, $reference, "$name/random_cond_var/$t", $abs, $rel);
            }
            continue;
        }
        $likelihood = in_array($key, ['log_likelihood', 'aic', 'bic'], true);
        if ($case['function'] === 'lmer') {
            [$abs, $rel] = $likelihood ? [5e-6, 1e-9] : [1e-5, 1e-5];
            $actual = $key === 'random_covariances' ? array_column($result['random'], 'covariance') : $result[$key];
        } else {
            [$abs, $rel] = $likelihood ? [1e-6, 1e-8] : (in_array($key, ['std_errors', 'covariance'], true) ? [2e-5, 2e-3] : [2e-5, 2e-4]);
            $actual = $result[$key];
        }
        mcompare($actual, $value, "$name/$key", $abs, $rel);
    }
    mcheck(array_keys($result['named']) === $expected['columns'], "$name: named");
    $fits[$case['function']]++;
}

// Formula fits equal the matrix interfaces they wrap.
$schools = $fixture['datasets']['schools'];
$lmer = Phit\fit_lmer('score ~ study_hours + (1 | school)', $schools, reml: false);
$matrix = Phit\fit_mixed(array_map(fn($h) => [$h], $schools['study_hours']), $schools['score'],
    [['z' => array_fill(0, count($schools['score']), [1]), 'groups' => $schools['school']]]);
mcompare($lmer['coefficients'], $matrix['coefficients'], 'lmer equals Phit\fit_mixed', 1e-12, 1e-12);
mcheck($lmer['method'] === 'ML' && $lmer['random'][0]['label'] === '1 | school' && $lmer['random'][0]['group_labels'] === $matrix['random'][0]['group_labels'], 'lmer metadata');
// Phit\fit_mixed_intercept (random-intercept ML) reports the same conditional variances as lme4.
$ml = array_values(array_filter($fixture['cases'], fn($c) => $c['name'] === 'lmer_schools_intercept_ml'))[0];
$lmm = Phit\fit_mixed_intercept(array_map(null, $schools['study_hours'], $schools['prior_score']), $schools['score'], $schools['school']);
cond_var_compare(array_map(fn($v) => [[$v]], $lmm['random_effects_cond_var']), $lmm['group_labels'],
    $ml['expected']['random_cond_var'][0], 'Phit\fit_mixed_intercept/random_effects_cond_var', 1e-5, 1e-5);
$panel = $fixture['datasets']['panel'];
$glmer = @Phit\fit_glmer('yb ~ x + (1 | subject)', $panel);
mcheck($glmer['family'] === 'binomial' && $glmer['random'][0]['group'] === 'subject' && $glmer['n_dropped'] === 2, 'glmer metadata');
foreach ([
    ['no random term', fn() => Phit\fit_lmer('score ~ study_hours', $schools), 1001, 'random-effect term'],
    ['glmer slopes', fn() => Phit\fit_glmer('yb ~ x + (x | subject)', $panel), 1007, 'exactly one random intercept'],
    ['glmer two terms', fn() => Phit\fit_glmer('yb ~ x + (1 | subject) + (1 | item)', $panel), 1007, 'exactly one random intercept'],
    ['glmer family', fn() => Phit\fit_glmer('yb ~ x + (1 | subject)', $panel, 'poisson'), 1007, 'binomial'],
    ['glmer link', fn() => Phit\fit_glmer('yb ~ x + (1 | subject)', $panel, link: 'probit'), 1007, 'logit'],
    ['lmer optimizer', fn() => Phit\fit_lmer('y ~ x + (1 | subject)', $panel, optimizer: 'nloptwrap'), 1007, 'optimizer'],
    ['lmer raise', fn() => Phit\fit_lmer('y ~ x + (1 | subject)', $panel, missing: 'raise'), 1001, "missing='raise'"],
] as [$label, $call, $code, $fragment]) {
    $thrown = null;
    try { @$call(); } catch (Throwable $e) { $thrown = $e; }
    mcheck($thrown !== null && $thrown->getCode() === $code && str_contains($thrown->getMessage(), $fragment), "$label: " . ($thrown?->getMessage() ?? 'no exception'));
}
$warnings = [];
set_error_handler(function (int $severity, string $message) use (&$warnings): bool { $warnings[] = $message; return true; });
try { $r = Phit\fit_lmer('y ~ x + (1 | subject)', $panel); } finally { restore_error_handler(); }
mcheck(count($warnings) === 1 && str_contains($warnings[0], 'Phit\fit_lmer dropped 5 of 384') && $r['dropped_rows'] === [4, 39, 76, 210, 299], 'lmer deletion warning');
echo "PHP formula mixed-model tests passed ({$fits['lmer']} lmer and {$fits['glmer']} glmer lme4 fits; $comparisons comparisons)\n";
