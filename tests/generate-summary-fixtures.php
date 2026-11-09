<?php

declare(strict_types=1);

// Writes tests/fixtures/summary/summary.json: the data, the model calls, the expected Phit\summary() text and
// Phit\tidy() columns, and R's printed coefficient rows (from tests/generate-summary-r.R). See the README there.

$root = dirname(__DIR__);
$dir = "$root/tests/fixtures/summary";

function columns(string $path): array
{
    $csv = Phit\read_csv($path);

    return array_combine($csv['names'], array_column($csv['columns'], 'values'));
}

$data = [
    'mtcars' => columns("$dir/mtcars.csv"),
    'mroz' => columns("$dir/mroz.csv"),
    'insurance' => columns("$dir/insurance.csv"),
    'sleepstudy' => columns("$root/docs/data/lmm/sleepstudy.csv"),
    'cbpp' => columns("$root/docs/data/cbpp.csv"),
];
// R reads cbpp's period as a factor (lme4 ships it as one); the CSV quotes it, but read_csv infers numbers.
$data['cbpp']['period'] = array_map('strval', $data['cbpp']['period']);
$iv = json_decode(file_get_contents("$root/tests/fixtures/iv-r.json"), true, flags: JSON_THROW_ON_ERROR);
$ivCase = array_values(array_filter($iv['cases'], fn ($c) => $c['name'] === 'one_over'))[0];
$data['iv'] = $ivCase['data'];
$cbpp = 'cbind(incidence, size - incidence) ~ period + (1 | herd)';
$cases = [
    ['iv_one_over', 'fit_iv', ['formula' => $ivCase['formula'], 'data' => 'iv']],
    ['ols_mtcars', 'fit_ols', ['formula' => 'mpg ~ wt + factor(cyl)', 'data' => 'mtcars']],
    ['gaussian_glm_mtcars', 'fit_generalized_linear', ['formula' => 'mpg ~ wt', 'data' => 'mtcars']],
    ['logistic_mroz', 'fit_generalized_linear',
        ['formula' => 'lfp ~ k5 + age + wc + inc', 'data' => 'mroz', 'family' => 'binomial']],
    ['probit_mroz', 'fit_generalized_linear',
        ['formula' => 'lfp ~ k5 + age + wc + inc', 'data' => 'mroz', 'family' => 'binomial', 'link' => 'probit']],
    ['poisson_insurance', 'fit_generalized_linear',
        ['formula' => 'Claims ~ factor(District) + offset(log(Holders))', 'data' => 'insurance', 'family' => 'poisson']],
    ['lmer_sleepstudy_reml', 'fit_mixed_gaussian', ['formula' => 'Reaction ~ Days + (Days | Subject)', 'data' => 'sleepstudy']],
    ['lmer_sleepstudy_ml', 'fit_mixed_gaussian',
        ['formula' => 'Reaction ~ Days + (1 | Subject)', 'data' => 'sleepstudy', 'reml' => false]],
    ['glmer_cbpp_laplace', 'fit_mixed_logistic', ['formula' => $cbpp, 'data' => 'cbpp']],
    ['glmer_cbpp_agq10', 'fit_mixed_logistic', ['formula' => $cbpp, 'data' => 'cbpp', 'quadraturePoints' => 10]],
    ['ols_matrix_mtcars', 'fit_ols_matrix',
        ['x' => array_map(fn ($w) => [$w], $data['mtcars']['wt']), 'y' => $data['mtcars']['mpg']]],
    ['logistic_matrix_mroz', 'fit_logistic_matrix', [
        'x' => array_map(null, $data['mroz']['k5'], $data['mroz']['age']),
        'y' => array_map(fn ($v) => $v === 'yes' ? 1 : 0, $data['mroz']['lfp']),
    ]],
    ['mixed_intercept_matrix_sleepstudy', 'fit_mixed_gaussian_intercept_matrix', [
        'x' => array_map(fn ($d) => [$d], $data['sleepstudy']['Days']),
        'y' => $data['sleepstudy']['Reaction'],
        'groups' => $data['sleepstudy']['Subject'],
    ]],
    ['probit_matrix_mroz', 'fit_probit_matrix', [
        'x' => array_map(null, $data['mroz']['k5'], $data['mroz']['age']),
        'y' => array_map(fn ($v) => $v === 'yes' ? 1 : 0, $data['mroz']['lfp']),
    ]],
    ['poisson_matrix_insurance', 'fit_poisson_matrix', [
        'x' => array_map(fn ($d) => [(int) ($d == 2), (int) ($d == 3), (int) ($d == 4)], $data['insurance']['District']),
        'y' => $data['insurance']['Claims'],
        'offset' => array_map('log', $data['insurance']['Holders']),
    ]],
    ['mixed_gaussian_matrix_sleepstudy', 'fit_mixed_gaussian_matrix', [
        'x' => array_map(fn ($d) => [$d], $data['sleepstudy']['Days']),
        'y' => $data['sleepstudy']['Reaction'],
        'random' => [[
            'z' => array_map(fn ($d) => [1, $d], $data['sleepstudy']['Days']),
            'groups' => $data['sleepstudy']['Subject'],
        ]],
        'reml' => true,
    ]],
    ['mixed_logistic_matrix_cbpp', 'fit_mixed_logistic_matrix', [
        'x' => array_map(fn ($p) => [(int) ($p === '2'), (int) ($p === '3'), (int) ($p === '4')], $data['cbpp']['period']),
        'y' => array_map(fn ($k, $n) => $k / $n, $data['cbpp']['incidence'], $data['cbpp']['size']),
        'groups' => $data['cbpp']['herd'],
        'weights' => $data['cbpp']['size'],
    ]],
];
// R has no matrix calls; these match the formula fits that have the same fixed effects.
$rName = [
    'mixed_intercept_matrix_sleepstudy' => 'lmer_sleepstudy_ml',
    'poisson_matrix_insurance' => 'poisson_insurance',
    'mixed_gaussian_matrix_sleepstudy' => 'lmer_sleepstudy_reml',
    'mixed_logistic_matrix_cbpp' => 'glmer_cbpp_laplace',
];
$r = json_decode(file_get_contents("$dir/summary-r.json"), true, flags: JSON_THROW_ON_ERROR);

$r['rows']['iv_one_over'] = $ivCase['r_rows'];
$out = ['R' => $r['R'], 'lme4' => $r['lme4'], 'data' => $data, 'cases' => []];
if (in_array('--iv-only', $argv, true)) {
    $out = json_decode(file_get_contents("$dir/summary.json"), true, flags: JSON_THROW_ON_ERROR);
    $out['data']['iv'] = $data['iv'];
    $out['cases'] = array_values(array_filter($out['cases'], fn ($c) => $c['name'] !== 'iv_one_over'));
    $cases = array_values(array_filter($cases, fn ($c) => $c[0] === 'iv_one_over'));
}
foreach ($cases as [$name, $function, $args]) {
    $call = $args;
    if (isset($call['data'])) {
        $call['data'] = $data[$call['data']];
    }
    $fit = ("Phit\\$function")(...$call);
    $out['cases'][] = [
        'name' => $name,
        'function' => $function,
        'args' => $args,
        'summary' => Phit\summary($fit),
        'tidy' => Phit\tidy($fit, confInt: true)->toArray(),
        'table' => (string) Phit\tidy($fit, confInt: true),
        'r_rows' => $r['rows'][$rName[$name] ?? $name],
    ];
    if (isset($r['tables'][$name])) {
        $out['cases'][array_key_last($out['cases'])]['r_table'] = $r['tables'][$name];
    }
    if (str_contains($function, 'mixed')) {
        $case = &$out['cases'][array_key_last($out['cases'])];
        $case['ran_vals'] = Phit\tidy($fit, effects: 'ran_vals')->toArray();
        $case['r_ran_vals'] = $r['ran_vals'][$rName[$name] ?? $name];
        unset($case);
    }
    echo "$name\n";
}
file_put_contents("$dir/summary.json", json_encode($out, JSON_PRETTY_PRINT | JSON_PRESERVE_ZERO_FRACTION
    | JSON_UNESCAPED_SLASHES | JSON_THROW_ON_ERROR)."\n");
