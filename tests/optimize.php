<?php
// Phit\optimize and Phit\uniroot against R's optimize() and uniroot()
// (tests/generate-optimize-fixtures.R): results bit for bit, and the same
// number of function evaluations.
declare(strict_types=1);
function check(bool $condition, string $message): void {
    if (!$condition) throw new RuntimeException($message);
}
function same(float|int $actual, float|int $expected, string $label): void {
    check((float) $actual === (float) $expected, sprintf('%s: %.17g !== %.17g', $label, $actual, $expected));
}
function throws(callable $call, string $label, string $class = Throwable::class): void {
    try {
        $call();
    } catch (Throwable $e) {
        check($e instanceof $class, "$label threw " . get_class($e));
        return;
    }
    throw new RuntimeException("$label did not throw");
}

// The PHP twins of the R functions in the generator.
$functions = [
    'parabola' => fn(float $x) => ($x - 1 / 3) * ($x - 1 / 3),
    'sine' => fn(float $x) => sin($x),
    'kink' => fn(float $x) => abs($x - 0.7),
    'flat' => fn(float $x) => 1.0,
    'linear' => fn(float $x) => $x,
    'decay' => fn(float $x) => exp(-$x),
    'double_well' => fn(float $x) => ($x * $x - 1) * ($x * $x - 1) + 0.3 * $x,
    'wall' => fn(float $x) => $x < 0.45 ? INF : ($x - 0.5) * ($x - 0.5),
    'far' => fn(float $x) => ($x - 1e6) * ($x - 1e6),
    'hump' => fn(float $x) => $x * exp(-$x),
    'sqrt2' => fn(float $x) => $x * $x - 2,
    'fixed_point' => fn(float $x) => cos($x) - $x,
    'exp5' => fn(float $x) => exp($x) - 5,
    'at_one' => fn(float $x) => $x - 1,
    'at_three' => fn(float $x) => $x - 3,
    'step' => fn(float $x) => $x < 0.3 ? -1.0 : 1.0,
    'cubic' => fn(float $x) => $x * $x * $x - 2 * $x - 5,
    'log' => fn(float $x) => log($x),
    'cliff' => fn(float $x) => $x < 0.5 ? -INF : $x - 0.6,
];
// Runs $call with $f counted, returning the result, the count and any warnings.
function counted(callable $f, callable $call): array {
    $calls = 0;
    $warnings = [];
    set_error_handler(function (int $no, string $message) use (&$warnings) {
        $warnings[] = $message;
        return true;
    });
    try {
        $result = $call(function (float $x) use ($f, &$calls) {
            $calls++;
            return $f($x);
        });
    } finally {
        restore_error_handler();
    }
    return [$result, $calls, $warnings];
}

$fixture = json_decode(file_get_contents(__DIR__ . '/fixtures/optimize-r.json'), true, flags: JSON_THROW_ON_ERROR);
$comparisons = 0;
foreach ($fixture['optimize'] as $i => $case) {
    $label = "optimize/$i {$case['f']}";
    $maximum = $case['maximum'] ?? false;
    [$r, $calls, $warnings] = counted($functions[$case['f']], fn($f) => Phit\optimize(
        $f, $case['lower'], $case['upper'], $case['tol'] ?? null, $maximum));
    $e = $case['expected'];
    check(array_keys($r) === [$maximum ? 'maximum' : 'minimum', 'objective'], "$label keys");
    same($r[$maximum ? 'maximum' : 'minimum'], $e['x'], "$label x");
    same($r['objective'], $e['objective'], "$label objective");
    same($calls, $e['evaluations'], "$label evaluations");
    check(count($warnings) === ($case['f'] === 'wall' ? 1 : 0), "$label warnings");
    $comparisons += 3;
}
foreach ($fixture['uniroot'] as $i => $case) {
    $label = "uniroot/$i {$case['f']}";
    [$r, $calls, $warnings] = counted($functions[$case['f']], fn($f) => Phit\uniroot(
        $f, $case['lower'], $case['upper'], $case['tol'] ?? null, $case['maxiter'] ?? 1000));
    $e = $case['expected'];
    check(array_keys($r) === ['root', 'f_root', 'iter', 'estim_prec'], "$label keys");
    check(is_int($r['iter']), "$label iter type");
    foreach (['root', 'f_root', 'iter', 'estim_prec'] as $key) same($r[$key], $e[$key], "$label $key");
    same($calls, $e['evaluations'], "$label evaluations");
    $notConverged = array_filter($warnings, fn($w) => str_contains($w, 'not converged'));
    check(($notConverged === []) === $e['converged'], "$label convergence warning");
    $comparisons += 5;
}

// f may be any callable; an exception from f propagates unchanged.
same(Phit\optimize('abs', -1, 2)['minimum'], Phit\optimize(fn($x) => abs($x), -1, 2)['minimum'], 'string callable');
throws(fn() => Phit\optimize(function () { throw new DomainException('stop'); }, 0, 1), 'optimize exception', DomainException::class);
throws(fn() => Phit\uniroot(function (float $x) { if ($x > 0.5) throw new DomainException('stop'); return $x - 0.75; }, 0, 1),
    'uniroot exception', DomainException::class);
// f's return values: ints are numbers, anything else is an error.
same(Phit\uniroot(fn($x) => $x < 2 ? -1 : 1, 0, 4)['f_root'], -1, 'int results');
throws(fn() => Phit\optimize(fn($x) => '1', 0, 1), 'string result');
throws(fn() => Phit\optimize(fn($x) => null, 0, 1), 'null result');

// Invalid input.
$sq = $functions['sqrt2'];
throws(fn() => Phit\optimize($sq, 1, 1), 'optimize empty interval');
throws(fn() => Phit\optimize($sq, 0, INF), 'optimize infinite upper');
throws(fn() => Phit\optimize($sq, 0, 1, tol: 0.0), 'optimize zero tol');
throws(fn() => Phit\uniroot($sq, 2, 0), 'uniroot reversed');
throws(fn() => Phit\uniroot($sq, 2, 3), 'uniroot same signs');
throws(fn() => Phit\uniroot(fn($x) => $x < 0.5 ? NAN : 1.0, 0, 1), 'uniroot NAN at lower');
throws(fn() => Phit\uniroot($sq, 0, 2, maxiter: 0), 'uniroot maxiter');
throws(fn() => Phit\uniroot($sq, 0, 2, tol: -1.0), 'uniroot negative tol');
echo "PHP optimize/uniroot tests passed ($comparisons bit-for-bit R comparisons, evaluation counts and validation)\n";
