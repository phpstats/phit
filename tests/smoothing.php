<?php
// stat_density, stat_bw_nrd0/nrd, stat_approx and stat_ecdf against R
// (tests/generate-smoothing-fixtures.R).
declare(strict_types=1);
function check(bool $condition, string $message): void {
    if (!$condition) throw new RuntimeException($message);
}
// Relative 1e-12; density heights are compared on the scale of their peak.
function close(float $actual, ?float $expected, string $label, float $scale = 0.0): void {
    if ($expected === null) {
        check(is_nan($actual), sprintf('%s: %.17g is not NAN', $label, $actual));
        return;
    }
    $tolerance = 1e-12 * max(abs($expected), $scale);
    check(abs($actual - $expected) <= $tolerance, sprintf('%s: %.17g != %.17g', $label, $actual, $expected));
}
function closeAll(array $actual, array $expected, string $label, float $scale = 0.0): void {
    check(array_is_list($actual) && count($actual) === count($expected), "$label shape");
    foreach ($expected as $i => $value) close($actual[$i], $value, "$label/$i", $scale);
}
function throws(callable $call, string $label): void {
    try {
        $call();
    } catch (Throwable) {
        return;
    }
    throw new RuntimeException("$label did not throw");
}

$fixture = json_decode(file_get_contents(__DIR__ . '/fixtures/smoothing-r.json'), true, flags: JSON_THROW_ON_ERROR);
$comparisons = 0;
foreach ($fixture['density'] as $case) {
    $d = Phit\stat_density($case['x'], ...$case['args']);
    $e = $case['expected'];
    check(array_keys($d) === ['x', 'y', 'bw', 'n'], "{$case['name']} keys");
    check($d['n'] === $e['n'], "{$case['name']} n");
    close($d['bw'], $e['bw'], "{$case['name']} bw");
    closeAll($d['x'], (array) $e['x'], "{$case['name']} x");
    closeAll($d['y'], (array) $e['y'], "{$case['name']} y", max((array) $e['y']));
    $comparisons += 2 + 2 * count((array) $e['y']);
}
foreach ($fixture['bandwidth'] as $case) {
    close(Phit\stat_bw_nrd0($case['x']), $case['nrd0'], "{$case['name']} nrd0");
    close(Phit\stat_bw_nrd($case['x']), $case['nrd'], "{$case['name']} nrd");
    $comparisons += 2;
}
foreach ($fixture['approx'] as $case) {
    $out = Phit\stat_approx($case['x'], $case['y'], $case['xout'], ...$case['args']);
    closeAll($out, $case['expected'], $case['name']);
    $comparisons += count($out);
}
foreach ($fixture['ecdf'] as $case) {
    $out = Phit\stat_ecdf($case['x'], $case['q']);
    check($out === array_map('floatval', $case['expected']), "{$case['name']} ecdf");
    $comparisons += count($out);
}

// Evaluation points: null and NAN give NAN; infinities are outside the data.
$out = Phit\stat_approx([1, 2], [10, 20], [null, NAN, -INF, INF], rule: 2);
check(is_nan($out[0]) && is_nan($out[1]) && $out[2] === 10.0 && $out[3] === 20.0, 'approx special points');
$out = Phit\stat_ecdf([1, 2], [null, -INF, INF]);
check(is_nan($out[0]) && $out[1] === 0.0 && $out[2] === 1.0, 'ecdf special points');

// Missing data: rejected by default, removed with missing: 'drop'.
throws(fn() => Phit\stat_density([1, null, 3]), 'density missing raise');
$dropped = Phit\stat_density([1.0, null, 3.0, NAN, 4.0], missing: 'drop');
check($dropped == Phit\stat_density([1.0, 3.0, 4.0]), 'density missing drop');
check(Phit\stat_approx([1, null, 3], [1, 5, 3], [2], missing: 'drop') === [2.0], 'approx missing drop');
check(Phit\stat_ecdf([1, NAN, 3], [2], missing: 'drop') === [0.5], 'ecdf missing drop');
// Weights that summed to one are renormalized after dropping, as R does.
$w = Phit\stat_density([1.0, null, 3.0], bw: 1.0, weights: [0.25, 0.25, 0.5], missing: 'drop');
check($w == Phit\stat_density([1.0, 3.0], bw: 1.0, weights: [1 / 3, 2 / 3]), 'density weights renormalized');

// Invalid input.
throws(fn() => Phit\stat_density([1, INF]), 'density infinite');
throws(fn() => Phit\stat_density([1]), 'automatic bandwidth needs two values');
throws(fn() => Phit\stat_density([1, 2], bw: 'ucv'), 'unsupported bandwidth rule');
throws(fn() => Phit\stat_density([1, 2], bw: -1.0), 'negative bandwidth');
throws(fn() => Phit\stat_density([1, 2], kernel: 'box'), 'unknown kernel');
throws(fn() => Phit\stat_density([1, 2], n: 0), 'n zero');
throws(fn() => Phit\stat_density([1, 2], from: 3.0, to: 1.0), 'from above to');
throws(fn() => Phit\stat_density([1, 2], weights: [1.0]), 'weights length');
throws(fn() => Phit\stat_density([1, 2], weights: [-1.0, 2.0]), 'negative weights');
throws(fn() => Phit\stat_approx([1, 2], [1, 2], [1], rule: 3), 'rule');
throws(fn() => Phit\stat_approx([1, 2], [1, 2], [1], method: 'spline'), 'method');
throws(fn() => Phit\stat_approx([1, 2], [1, 2], [1], ties: 'ordered'), 'ties');
throws(fn() => Phit\stat_approx([1, 1], [1, 2], [1]), 'linear needs two distinct x');
throws(fn() => Phit\stat_approx([1, 2], [1], [1]), 'approx lengths');
throws(fn() => Phit\stat_ecdf([], [1]), 'ecdf empty');
echo "PHP smoothing tests passed ($comparisons R comparisons, missing policies and validation)\n";
