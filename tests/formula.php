<?php
declare(strict_types=1);
// Formula interfaces versus R model.matrix()/lm()/glm() (tests/generate-formula-fixtures.R).
function fcheck(bool $condition, string $message): void {
    if (!$condition) throw new RuntimeException($message);
}
$comparisons = 0;
function fcompare(mixed $actual, mixed $expected, string $label, float $abs, float $rel): void {
    global $comparisons;
    if (is_array($expected)) {
        fcheck(is_array($actual) && count($actual) === count($expected), "$label shape");
        foreach ($expected as $i => $value) fcompare($actual[$i] ?? null, $value, "$label/$i", $abs, $rel);
        return;
    }
    if (is_string($expected) || is_bool($expected)) {
        fcheck($actual === $expected, "$label: " . var_export($actual, true) . ' !== ' . var_export($expected, true));
    } else {
        fcheck(is_int($actual) || is_float($actual), "$label: not numeric");
        fcheck(abs($actual - $expected) <= $abs + $rel * abs($expected), "$label: $actual != $expected");
    }
    $comparisons++;
}
/** Run a call, returning [result, warnings]. */
function fwarnings(callable $call): array {
    $warnings = [];
    set_error_handler(function (int $severity, string $message) use (&$warnings): bool {
        $warnings[] = [$severity, $message];
        return true;
    });
    try { $result = $call(); } finally { restore_error_handler(); }
    return [$result, $warnings];
}
function fthrows(callable $call, int $code, string $fragment, string $label): void {
    try {
        fwarnings($call);
    } catch (Throwable $e) {
        fcheck($e->getCode() === $code, "$label: code {$e->getCode()} ({$e->getMessage()}) != $code");
        fcheck(str_contains($e->getMessage(), $fragment), "$label: message '{$e->getMessage()}' lacks '$fragment'");
        return;
    }
    throw new RuntimeException("$label: expected exception $code");
}

$fixture = json_decode(file_get_contents(__DIR__ . '/fixtures/formula-r.json'), true, flags: JSON_THROW_ON_ERROR);
$counts = ['model_matrix' => 0, 'lm' => 0, 'glm' => 0];
foreach ($fixture['cases'] as $case) {
    $data = $fixture['datasets'][$case['dataset']];
    $name = $case['name'];
    $expected = $case['expected'];
    [$result, $warnings] = fwarnings(fn() => match ($case['function']) {
        'model_matrix' => Phit\model_matrix($case['formula'], $data),
        'lm' => Phit\fit_lm($case['formula'], $data, weights: $case['weights']),
        'glm' => Phit\fit_glm($case['formula'], $data, $case['family'], $case['link'], weights: $case['weights']),
    });
    $dropped = count($data[array_key_first($data)]) - count($expected['retained_rows']);
    if (array_key_exists('codes', $data[array_key_first($data)])) $dropped = count($data[array_key_first($data)]['codes']) - count($expected['retained_rows']);
    fcheck(count($warnings) === ($dropped > 0 ? 1 : 0), "$name: one warning per deleting fit");
    if ($dropped > 0) fcheck($warnings[0][0] === E_WARNING && str_contains($warnings[0][1], "dropped $dropped of"), "$name: warning text");
    fcheck($result['retained_rows'] === $expected['retained_rows'], "$name: retained rows");
    fcheck($result['n_dropped'] === $dropped, "$name: dropped count");
    fcheck($result['columns'] === $expected['columns'], "$name: columns " . json_encode($result['columns']) . ' vs ' . json_encode($expected['columns']));
    fcheck($result['terms'] === $expected['terms'], "$name: terms " . json_encode($result['terms']));
    foreach ($expected as $key => $value) {
        if (in_array($key, ['columns', 'terms', 'retained_rows'], true)) continue;
        fcheck(array_key_exists($key, $result), "$name: missing key $key");
        // Design values are exact up to rounding; fit tolerances match the matrix suites.
        [$abs, $rel] = $case['function'] === 'model_matrix' ? [1e-12, 1e-12] : [1e-9, 1e-7];
        if (str_contains($case['formula'], 'poly(area_m2, 2)')) [$abs, $rel] = [1e-9, 1e-7];
        fcompare($result[$key], $value, "$name/$key", $abs, $rel);
    }
    if ($case['function'] !== 'model_matrix') {
        fcheck(array_keys($result['named']) === $expected['columns'], "$name: named keys");
        fcheck(array_values($result['named']) === $result['coefficients'], "$name: named values");
        fcheck($result['diagnostics']['converged'] === true, "$name: diagnostics");
    }
    $counts[$case['function']]++;
}
fcheck($counts['model_matrix'] >= 30 && $counts['lm'] >= 12 && $counts['glm'] >= 12, 'fixture coverage');

// Errors use the extension's stable codes.
$syn = $fixture['datasets']['synthetic'];
$base = ['y' => [1.5, 2.1, 2.9, 4.2, 4.8, 6.1], 'x' => [1, 2, 3, 4, 5, 6], 'g' => ['a', 'b', 'a', 'b', 'c', 'c']];
foreach ([
    ['unknown column', fn() => Phit\fit_lm('y ~ nope', $base), 1001, "unknown column 'nope'"],
    ['length mismatch', fn() => Phit\fit_lm('y ~ x', $base + ['z' => [1, 2]]), 1002, "column 'z'"],
    ['list column', fn() => Phit\fit_lm('y ~ x + z', $base + ['z' => [[1], [2], [3], [4], [5], [6]]]), 1001, 'list-columns'],
    ['mixed column', fn() => Phit\fit_lm('y ~ x + z', $base + ['z' => [1, 'a', 2, 3, 4, 5]]), 1001, 'mix'],
    ['descriptor keys', fn() => Phit\fit_lm('y ~ f', $base + ['f' => ['levels' => ['a'], 'codes' => [0, 0, 0, 0, 0, 0], 'ordered' => false, 'x' => 1]]), 1001, 'factor descriptor'],
    ['descriptor code', fn() => Phit\fit_lm('y ~ f', $base + ['f' => ['levels' => ['a', 'b'], 'codes' => [0, 1, 2, 0, 1, 0], 'ordered' => false]]), 1001, '0-based'],
    ['descriptor levels', fn() => Phit\fit_lm('y ~ f', $base + ['f' => ['levels' => [1, 2], 'codes' => [0, 1, 1, 0, 1, 0], 'ordered' => false]]), 1001, 'strings'],
    ['descriptor ordered', fn() => Phit\fit_lm('y ~ f', $base + ['f' => ['levels' => ['a', 'b'], 'codes' => [0, 1, 1, 0, 1, 0], 'ordered' => 'no']]), 1001, 'boolean'],
    ['duplicate levels', fn() => Phit\fit_lm('y ~ f', $base + ['f' => ['levels' => ['a', 'a'], 'codes' => [0, 1, 1, 0, 1, 0], 'ordered' => false]]), 1001, 'duplicate levels'],
    ['row list data', fn() => Phit\fit_lm('y ~ x', [['y' => 1, 'x' => 2]]), 1001, 'associative array of columns'],
    ['scalar data', fn() => Phit\fit_lm('y ~ x', 5), 1001, 'toModelData'],
    ['raise', fn() => Phit\fit_lm('y ~ x1', $syn, 'raise'), 1001, "missing='raise'"],
    ['model matrix raise', fn() => Phit\model_matrix('y ~ x1', $syn, 'raise'), 1001, "missing='raise'"],
    ['policy', fn() => Phit\fit_lm('y ~ x', $base, 'keep'), 1001, "'drop' or 'raise'"],
    ['syntax', fn() => Phit\fit_lm('y ~ x +', $base), 1001, 'formula'],
    ['no tilde', fn() => Phit\fit_lm('y + x', $base), 1001, "'~'"],
    ['bare bar', fn() => Phit\model_matrix('y ~ x + log(x | g)', $base), 1001, ''],
    ['unsupported function', fn() => Phit\fit_lm('y ~ ns(x, 3)', $base), 1007, "'ns()'"],
    ['random in lm', fn() => Phit\fit_lm('y ~ x + (1 | g)', $base), 1007, 'Phit\fit_lmer'],
    ['random in glm', fn() => Phit\fit_glm('y ~ x + (1 | g)', $base), 1007, 'Phit\fit_mixed'],
    ['family', fn() => Phit\fit_glm('y ~ x', $base, 'gamma'), 1007, "family 'gamma'"],
    ['link', fn() => Phit\fit_glm('y ~ x', $base, 'poisson', 'identity'), 1007, "link 'identity'"],
    ['rank deficiency', fn() => Phit\fit_lm('y ~ x + I(2 * x)', $base), 1004, ''],
    ['unused level with intercept-free factor', fn() => Phit\fit_lm('y ~ 0 + g + I(g == "a")', $base), 1004, ''],
    ['binomial range', fn() => Phit\fit_glm('y ~ x', $base, 'binomial'), 1001, '[0,1]'],
    ['multi-level response in lm', fn() => Phit\fit_lm('g ~ x', $base), 1001, 'numeric or logical response'],
    ['poisson counts', fn() => Phit\fit_glm('y ~ x', $base, 'poisson'), 1001, 'nonnegative integer'],
    ['one-sided lm', fn() => Phit\fit_lm('~ x', $base), 1001, 'response'],
    ['empty design', fn() => Phit\fit_lm('y ~ 0', $base), 1001, 'no predictor columns'],
    ['log zero', fn() => Phit\fit_lm('y ~ log(x - 1)', $base), 1003, 'infinite'],
    ['infinite data', fn() => Phit\fit_lm('y ~ x', ['y' => [1, 2, 3], 'x' => [1, INF, 3]]), 1003, 'infinite'],
    ['single level contrast', fn() => Phit\fit_lm('y ~ x + f', $base + ['f' => ['u', 'u', 'u', 'u', 'u', 'u']]), 1001, '2 or more levels'],
    ['weights length', fn() => Phit\fit_lm('y ~ x', $base, weights: [1, 2]), 1002, 'one value per'],
    ['weights sign', fn() => Phit\fit_lm('y ~ x', $base, weights: [1, 2, 0, 1, 1, 1]), 1001, 'positive'],
    ['weights column', fn() => Phit\fit_lm('y ~ x', $base, weights: 'g'), 1001, 'numeric'],
    ['weights unknown', fn() => Phit\fit_lm('y ~ x', $base, weights: 'nope'), 1001, "unknown weights column"],
    ['orthogonal poly NA', fn() => Phit\fit_lm('y ~ poly(x2, 2)', $syn), 1001, 'raw = TRUE'],
    ['binomial weights', fn() => Phit\fit_glm('I(x / 10) ~ y', $base, 'binomial', weights: [1.5, 1, 1, 1, 1, 1]), 1001, 'integer'],
] as [$label, $call, $code, $fragment]) fthrows($call, $code, $fragment, $label);

// Unused columns may be anything a model cannot use; only referenced columns are validated.
$withList = $base + ['meta' => [[1], [2], [3], [4], [5], [6]], 'mixed' => [1, 'a', true, null, 2, 3]];
fcompare(Phit\fit_lm('y ~ x', $withList)['coefficients'], Phit\fit_lm('y ~ x', $base)['coefficients'], 'unused list column', 0, 0);

// Weights: a column name and an aligned array are equivalent; null weights drop rows.
$wdata = $base + ['w' => [1, 2, 1, 3, 1, 2]];
fcompare(Phit\fit_lm('y ~ x', $wdata, weights: 'w')['coefficients'], Phit\fit_lm('y ~ x', $base, weights: [1, 2, 1, 3, 1, 2])['coefficients'], 'weights by name', 1e-14, 1e-14);
[$wnull, $warn] = fwarnings(fn() => Phit\fit_lm('y ~ x', $base, weights: [1, 2, null, 3, 1, 2]));
fcheck($wnull['dropped_rows'] === [2] && count($warn) === 1 && str_contains($warn[0][1], 'Phit\fit_lm dropped 1 of 6'), 'null weight deletion');
fcheck(Phit\fit_ols(array_map(fn($v) => [$v], $base['x']), $base['y'])['coefficients'] === Phit\fit_lm('y ~ x', $base)['coefficients'], 'Phit\fit_lm equals Phit\fit_ols');

// Deletion uses only the variables the formula needs.
$sparse = $base + ['z' => [null, null, null, null, null, null]];
[$lm, $warn] = fwarnings(fn() => Phit\fit_lm('y ~ x', $sparse));
fcheck($lm['n_dropped'] === 0 && $warn === [], 'unused missing column is ignored');
[$mm, $warn] = fwarnings(fn() => Phit\model_matrix('y ~ x1 + f', $syn));
fcheck($mm['n_input'] === 90 && $mm['n_observations'] === count($mm['x']) && $mm['n_observations'] + $mm['n_dropped'] === 90, 'model matrix accounting');
fcheck(count($warn) === 1 && $warn[0][0] === E_WARNING && str_contains($warn[0][1], 'Phit\model_matrix dropped'), 'model matrix warning');
set_error_handler(function (int $severity, string $message): never { throw new ErrorException($message, 0, $severity); });
$promoted = false;
try { Phit\fit_lm('y ~ x1', $syn); } catch (ErrorException $e) { $promoted = true; } finally { restore_error_handler(); }
fcheck($promoted, 'warning promotion');

// glm conveniences: logical and factor responses, R's first-level-failure rule, names.
$bdata = ['ok' => [true, false, true, true, false, true, false, false], 'x' => [1, 2, 3, 4, 5, 6, 7, 8],
    'res' => ['levels' => ['fail', 'pass'], 'codes' => [1, 0, 1, 1, 0, 1, 0, 0], 'ordered' => false],
    'txt' => ['pass', 'fail', 'pass', 'pass', 'fail', 'pass', 'fail', 'fail']];
$logical = Phit\fit_glm('ok ~ x', $bdata, 'binomial');
fcompare(Phit\fit_glm('res ~ x', $bdata, 'binomial')['coefficients'], $logical['coefficients'], 'factor response', 1e-12, 1e-12);
fcompare(Phit\fit_glm('txt ~ x', $bdata, 'binomial')['coefficients'], $logical['coefficients'], 'character response', 1e-12, 1e-12);
fcompare(Phit\fit_logistic(array_map(fn($v) => [$v], $bdata['x']), [1, 0, 1, 1, 0, 1, 0, 0])['coefficients'], $logical['coefficients'], 'glm equals Phit\fit_logistic', 0, 0);
fcheck($logical['family'] === 'binomial' && $logical['link'] === 'logit' && $logical['diagnostics']['solver'] === 'newton_irls', 'family metadata');
fcheck(Phit\fit_glm('ok ~ x', $bdata, 'binomial', 'probit')['diagnostics']['solver'] === 'fisher_scoring', 'probit solver');
fcheck($logical['formula'] === 'ok ~ x' && $logical['response'] === 'ok' && $logical['assign'] === [0, 1] && $logical['intercept'] === true, 'naming metadata');

// Object data: toModelData() first, then toColumns(); method exceptions propagate.
final class ModelDataSource { public function __construct(private array $c) {} public function toModelData(): array { return $this->c; } }
final class ColumnsSource { public function __construct(private array $c) {} public function toColumns(): array { return $this->c; } }
final class ThrowingSource { public function toModelData(): array { throw new DomainException('source failed', 77); } }
fcompare(Phit\fit_lm('y ~ x + g', new ModelDataSource($base))['coefficients'], Phit\fit_lm('y ~ x + g', $base)['coefficients'], 'toModelData', 0, 0);
fcompare(Phit\model_matrix('y ~ x + g', new ColumnsSource($base))['x'], Phit\model_matrix('y ~ x + g', $base)['x'], 'toColumns', 0, 0);
fthrows(fn() => Phit\fit_lm('y ~ x', new ArrayObject($base)), 1001, 'toModelData', 'object without methods');
$caught = null;
try { Phit\fit_lm('y ~ x', new ThrowingSource()); } catch (DomainException $e) { $caught = $e; }
fcheck($caught !== null && $caught->getCode() === 77, 'toModelData exception propagates');

// Random-effect bars: explicit z/group designs in Phit\fit_mixed's shape.
$schools = $fixture['datasets']['schools'];
$mm = Phit\model_matrix('score ~ study_hours + (1 + study_hours | school) + (0 + prior_score || school)', $schools);
fcheck($mm['columns'] === ['(Intercept)', 'study_hours'] && count($mm['random']) === 2, 'bars removed from fixed design');
[$slopes, $independent] = $mm['random'];
fcheck($slopes['label'] === '1 + study_hours | school' && $slopes['group'] === 'school' && $slopes['correlated'] === true, 'bar metadata');
fcheck($slopes['columns'] === ['(Intercept)', 'study_hours'] && $independent['columns'] === ['prior_score'] && $independent['correlated'] === false, 'bar columns');
fcheck($slopes['groups'] === $schools['school'] && $slopes['z'][3] === [1.0, (float)$schools['study_hours'][3]], 'bar rows');
$direct = Phit\fit_mixed(x: array_map(fn($h) => [$h], $schools['study_hours']), y: $schools['score'],
    random: [['z' => array_map(fn($h) => [1, $h], $schools['study_hours']), 'groups' => $schools['school']]]);
$through = Phit\fit_mixed(x: $mm['x'], y: $mm['y'], random: [$slopes], intercept: false);
fcompare($through['coefficients'], $direct['coefficients'], 'model matrix to Phit\fit_mixed', 1e-10, 1e-10);
$nested = Phit\model_matrix('y ~ x + (1 | g/f) + (x | g:f)', $base + ['f' => [1, 1, 2, 2, 1, 2]]);
fcheck(array_column($nested['random'], 'label') === ['1 | g', '1 | f:g', 'x | g:f'], 'nested bars expand like lme4');
fcheck($nested['random'][1]['groups'][0] === '1:a' && $nested['random'][2]['groups'][0] === 'a:1' && $nested['random'][0]['groups'][0] === 'a', 'interaction group labels');
fcheck(Phit\model_matrix('y ~ x + (1 | x)', $base)['random'][0]['groups'][0] === 1, 'integer group labels stay integers');

// Optional integration with a real phibble DataFrame when PHIBBLE_AUTOLOAD names its autoloader.
$autoload = getenv('PHIBBLE_AUTOLOAD');
if (is_string($autoload) && is_file($autoload)) {
    require $autoload;
    $df = \Phibble\DataFrame::fromColumns(['y' => $base['y'], 'x' => $base['x'], 'g' => $base['g']], ['g' => 'factor']);
    fcompare(Phit\fit_lm('y ~ x + g', $df)['coefficients'], Phit\fit_lm('y ~ x + g', $base)['coefficients'], 'phibble DataFrame', 1e-14, 1e-14);
    fcheck(Phit\fit_lm('y ~ x + g', $df)['columns'] === ['(Intercept)', 'x', 'gb', 'gc'], 'phibble factor names');
    echo "phibble DataFrame integration passed\n";
}
echo "PHP formula tests passed ({$counts['model_matrix']} model.matrix, {$counts['lm']} lm, {$counts['glm']} glm fits; $comparisons R comparisons)\n";
