<?php
// The 0.3 names are gone (removed in 0.5.0), and fit_ols takes only a formula.
declare(strict_types=1);

function alias_check(bool $ok, string $label): void
{
    if (!$ok) {
        throw new RuntimeException($label);
    }
}

function load(string $path): array
{
    $csv = Phit\read_csv(__DIR__ . '/../docs/data/' . $path);
    return array_combine($csv['names'], array_column($csv['columns'], 'values'));
}

$schools = load('schools.csv');

$x = array_map(fn ($h, $p) => [$h, $p], $schools['study_hours'], $schools['prior_score']);
$y = $schools['score'];

// The 0.3 names were deprecated in 0.4.0 and removed in 0.5.0.
$removed = ['fit_lm', 'fit_glm', 'fit_lmer', 'fit_glmer', 'fit_logistic', 'fit_probit', 'fit_poisson',
    'fit_mixed_intercept', 'fit_mixed', 'fit_logistic_mixed'];
foreach ($removed as $old) {
    alias_check(!function_exists("Phit\\$old"), "$old is removed");
}
$parameters = array_map(fn ($p) => $p->getName(), (new ReflectionFunction('Phit\fit_mixed_logistic'))->getParameters());
alias_check(!in_array('family', $parameters, true) && !in_array('link', $parameters, true), 'fit_mixed_logistic has no family or link');

// fit_ols takes only a formula: a matrix goes to fit_ols_matrix.
$formula = Phit\fit_ols('score ~ study_hours + prior_score', $schools);
alias_check($formula === Phit\fit_ols(data: $schools, formula: 'score ~ study_hours + prior_score'), 'fit_ols named arguments');
alias_check(Phit\fit_ols_matrix($x, $y)['coefficients'] === $formula['coefficients'], 'matrix and formula fits agree exactly');
$parameters = array_map(fn ($p) => $p->getName(), (new ReflectionFunction('Phit\fit_ols'))->getParameters());
alias_check($parameters === ['formula', 'data', 'missing', 'weights'], 'fit_ols parameters: ' . json_encode($parameters));
foreach (['matrix' => fn () => Phit\fit_ols($x, $y), 'named x' => fn () => Phit\fit_ols(x: $x, y: $y)] as $label => $call) {
    $thrown = null;
    try {
        $call();
    } catch (Throwable $e) {
        $thrown = $e;
    }
    alias_check($thrown !== null, "fit_ols $label is rejected");
}

echo 'PHP names: ' . count($removed) . " removed aliases, and fit_ols takes only a formula\n";
