<?php
// phit 0.4 transition: every 0.3 name is a deprecated alias that returns exactly what its replacement returns,
// with one E_USER_DEPRECATED per call, and fit_ols dispatches on its first argument without changing any call's meaning.
declare(strict_types=1);

function alias_check(bool $ok, string $label): void
{
    if (!$ok) {
        throw new RuntimeException($label);
    }
}

/** Runs $call, returning its result and the E_USER_DEPRECATED messages it raised. */
function deprecations(callable $call): array
{
    $messages = [];
    set_error_handler(function (int $level, string $message) use (&$messages): bool {
        if ($level === E_USER_DEPRECATED) {
            $messages[] = $message;
            return true;
        }
        return false;
    });
    try {
        $result = $call();
    } finally {
        restore_error_handler();
    }
    return [$result, $messages];
}

function load(string $path, array $options = []): array
{
    $csv = Phit\read_csv(__DIR__ . '/../docs/data/' . $path, $options);
    return array_combine($csv['names'], array_column($csv['columns'], 'values'));
}

$schools = load('schools.csv');
$herds = load('cbpp.csv', ['types' => ['c', 'i', 'i', 'c']]);
$herds['healthy'] = array_map(fn ($n, $k) => $n - $k, $herds['size'], $herds['incidence']);

$x = array_map(fn ($h, $p) => [$h, $p], $schools['study_hours'], $schools['prior_score']);
$y = $schools['score'];
$pass = array_map(fn ($s) => $s >= 70.0 ? 1 : 0, $y);
$count = array_map(fn ($s) => (int) floor($s / 10), $y);
$schools['pass'] = $pass;
$groups = $schools['school'];
$weights = array_map(fn ($i) => 1.0 + ($i % 3) / 2, array_keys($y));
$random = [['z' => array_fill(0, count($y), [1.0]), 'groups' => $groups]];
$hx = array_map(fn ($p) => [$p === '2' ? 1.0 : 0.0, $p === '3' ? 1.0 : 0.0, $p === '4' ? 1.0 : 0.0], $herds['period']);
$hy = array_map(fn ($k, $n) => $k / $n, $herds['incidence'], $herds['size']);

// [old name, new name, arguments]: the alias must return exactly the replacement's result.
$cases = [
    ['fit_lm', 'fit_ols', ['score ~ study_hours + prior_score', $schools], ['weights' => $weights]],
    ['fit_glm', 'fit_generalized_linear', ['pass ~ study_hours + prior_score', $schools], ['family' => 'binomial', 'link' => 'probit']],
    ['fit_lmer', 'fit_mixed_gaussian', ['score ~ study_hours + (1 | school)', $schools], ['reml' => false]],
    ['fit_glmer', 'fit_mixed_logistic', ['cbind(incidence, healthy) ~ period + (1 | herd)', $herds], ['quadraturePoints' => 5]],
    ['fit_logistic', 'fit_logistic_matrix', [$x, $pass], ['offset' => array_fill(0, count($pass), 0.1)]],
    ['fit_probit', 'fit_probit_matrix', [$x, $pass], []],
    ['fit_poisson', 'fit_poisson_matrix', [$x, $count], ['intercept' => true]],
    ['fit_mixed_intercept', 'fit_mixed_gaussian_intercept_matrix', [$x, $y, $groups], []],
    ['fit_mixed', 'fit_mixed_gaussian_matrix', [$x, $y, $random], ['reml' => true]],
    ['fit_logistic_mixed', 'fit_mixed_logistic_matrix', [$hx, $hy, $herds['herd']], ['weights' => $herds['size']]],
];
foreach ($cases as [$old, $new, $positional, $named]) {
    [$expected, $none] = deprecations(fn () => ("Phit\\$new")(...$positional, ...$named));
    alias_check($none === [], "$new raises no deprecation");
    [$actual, $messages] = deprecations(fn () => ("Phit\\$old")(...$positional, ...$named));
    alias_check($actual === $expected, "$old returns exactly $new's result");
    alias_check(
        $messages === ["Phit\\$old(): Deprecated since phit 0.4.0; use Phit\\$new() instead"],
        "$old raises one deprecation naming $new: " . json_encode($messages)
    );
}

// fit_glmer keeps its family and link arguments; fit_mixed_logistic has neither.
[$glmer] = deprecations(fn () => Phit\fit_glmer('cbind(incidence, healthy) ~ period + (1 | herd)', $herds, 'binomial', 'logit'));
alias_check($glmer === Phit\fit_mixed_logistic('cbind(incidence, healthy) ~ period + (1 | herd)', $herds), 'fit_glmer binomial/logit');
$parameters = array_map(fn ($p) => $p->getName(), (new ReflectionFunction('Phit\fit_mixed_logistic'))->getParameters());
alias_check(!in_array('family', $parameters, true) && !in_array('link', $parameters, true), 'fit_mixed_logistic has no family or link');

// fit_ols: a formula (positional or named) is the formula fit, with no deprecation.
$formula = Phit\fit_ols('score ~ study_hours + prior_score', $schools);
foreach ([
    'positional formula' => fn () => Phit\fit_ols('score ~ study_hours + prior_score', $schools),
    'named formula' => fn () => Phit\fit_ols(formula: 'score ~ study_hours + prior_score', data: $schools),
    'named formula, reversed' => fn () => Phit\fit_ols(data: $schools, formula: 'score ~ study_hours + prior_score'),
] as $label => $call) {
    [$result, $messages] = deprecations($call);
    alias_check($result === $formula && $messages === [], "fit_ols $label");
}
alias_check($formula['columns'] === ['(Intercept)', 'study_hours', 'prior_score'], 'fit_ols formula names');

// An array (positional or named x:/y:) is the 0.3 matrix fit, with one deprecation.
$matrix = Phit\fit_ols_matrix($x, $y, weights: $weights);
foreach ([
    'positional matrix' => fn () => Phit\fit_ols($x, $y, weights: $weights),
    'named matrix' => fn () => Phit\fit_ols(x: $x, y: $y, weights: $weights),
    'positional x, named y' => fn () => Phit\fit_ols($x, y: $y, weights: $weights),
    'all positional' => fn () => Phit\fit_ols($x, $y, 'drop', $weights, null, true),
] as $label => $call) {
    [$result, $messages] = deprecations($call);
    alias_check($result === $matrix, "fit_ols $label returns fit_ols_matrix's result");
    alias_check(
        $messages === ['Phit\fit_ols(): Passing a matrix is deprecated since phit 0.4.0; use Phit\fit_ols_matrix() instead'],
        "fit_ols $label deprecation: " . json_encode($messages)
    );
}
[$result] = deprecations(fn () => Phit\fit_ols($x, $y, intercept: false, offset: array_fill(0, count($y), 1.0)));
alias_check($result === Phit\fit_ols_matrix($x, $y, intercept: false, offset: array_fill(0, count($y), 1.0)), 'fit_ols matrix offset and intercept');

// The same numbers as the formula fit, from the formula's model matrix.
alias_check($matrix['coefficients'] !== [] && Phit\fit_ols_matrix($x, $y)['coefficients'] === $formula['coefficients'], 'matrix and formula fits agree exactly');

// Mixed or incomplete arguments are input errors (1001) and raise no deprecation.
foreach ([
    'formula with x' => [fn () => Phit\fit_ols('score ~ study_hours', $schools, x: $x), 'a formula and data, or a matrix x and y, not both'],
    'array with x' => [fn () => Phit\fit_ols($x, x: $x, y: $y), 'a formula and data, or a matrix x and y, not both'],
    'data with y' => [fn () => Phit\fit_ols(x: $x, data: $y, y: $y), 'a formula and data, or a matrix x and y, not both'],
    'formula with offset' => [fn () => Phit\fit_ols('score ~ study_hours', $schools, offset: $y), 'no offset or intercept arguments'],
    'formula without intercept' => [fn () => Phit\fit_ols('score ~ study_hours', $schools, intercept: false), 'no offset or intercept arguments'],
    'formula without data' => [fn () => Phit\fit_ols('score ~ study_hours'), 'with a formula needs data'],
    'nothing' => [fn () => Phit\fit_ols(), 'needs a formula and data'],
    'x without y' => [fn () => Phit\fit_ols($x), 'needs a formula and data'],
    'number' => [fn () => Phit\fit_ols(5, $y), 'needs a formula string, or a matrix x as an array'],
    'matrix with string y' => [fn () => Phit\fit_ols($x, 'score'), 'needs y as an array'],
    'matrix with column-name weights' => [fn () => Phit\fit_ols($x, $y, weights: 'w'), 'needs weights as an array'],
] as $label => [$call, $fragment]) {
    $thrown = null;
    [, $messages] = deprecations(function () use ($call, &$thrown) {
        try {
            $call();
        } catch (Exception $e) {
            $thrown = $e;
        }
    });
    alias_check($thrown !== null && $thrown->getCode() === 1001 && str_contains($thrown->getMessage(), "Phit\\fit_ols") && str_contains($thrown->getMessage(), $fragment),
        "fit_ols $label: " . ($thrown?->getMessage() ?? 'no exception'));
    alias_check($messages === [], "fit_ols $label raises no deprecation");
}

echo 'PHP 0.4 names: ' . count($cases) . " deprecated aliases and fit_ols dispatch passed\n";
