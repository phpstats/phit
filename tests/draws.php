<?php
declare(strict_types=1);

function check(bool $ok, string $message): void {
    if (!$ok) throw new RuntimeException($message);
}
function closeValues(mixed $actual, mixed $expected, string $name): void {
    if (is_array($expected)) {
        check(is_array($actual) && array_keys($actual) === array_keys($expected), "$name shape/keys");
        foreach ($expected as $key => $v) closeValues($actual[$key], $v, "$name.$key");
    } else {
        check(is_finite((float)$actual) && abs($actual - $expected) <= 1e-11 + 1e-11 * abs($expected),
              "$name: $actual != $expected");
    }
}
function code(callable $f, int $expected): void {
    try { $f(); } catch (Throwable $e) {
        check($e->getCode() === $expected, "code {$e->getCode()} != $expected: {$e->getMessage()}");
        return;
    }
    throw new RuntimeException("Expected exception $expected");
}
function transpose(array $a): array {
    $out = [];
    for ($j = 0; $j < count($a[0]); $j++) $out[] = array_column($a, $j);
    return $out;
}
function multiply(array $a, array $b): array {
    $out = [];
    foreach ($a as $i => $row) {
        foreach (transpose($b) as $j => $col) {
            $out[$i][$j] = array_sum(array_map(fn($x, $y) => $x * $y, $row, $col));
        }
    }
    return $out;
}
$fixture = json_decode(file_get_contents(__DIR__.'/fixtures/draws-python.json'), true, flags: JSON_THROW_ON_ERROR);
foreach ($fixture['cases'] as $case) {
    $name = $case['name'];
    switch ($case['operation']) {
        case 'moments':
            $result = Phit\draws_moments(draws: $case['draws'], weights: $case['weights'], normalization: $case['normalization']);
            closeValues($result, $case['expected'], $name);
            break;
        case 'covariance_map':
            $result = Phit\draws_covariance_map(sourceCovariance: $case['source_covariance'], targetCovariance: $case['target_covariance']);
            closeValues($result, $case['expected'], $name);
            $a = $result['matrix'];
            $cov = multiply(multiply($a, $case['source_covariance']), transpose($a));
            closeValues($cov, $case['target_covariance'], "$name identity");
            foreach ($cov as $i => $row) foreach ($row as $j => $v) {
                $ref = $case['target_covariance'][$i][$j];
                if ($ref != 0) check(abs($v / $ref - 1) < 1e-12, "$name relative identity");
            }
            $reverse = Phit\draws_covariance_map($case['target_covariance'], $case['source_covariance']);
            closeValues($result['log_abs_det'] + $reverse['log_abs_det'], 0, "$name reverse logdet");
            break;
        case 'affine':
            $result = Phit\draws_affine(draws: $case['draws'], matrix: $case['matrix'], shift: $case['shift']);
            closeValues($result, $case['expected'], $name);
            foreach ($result['draws'] as $i => $row) foreach ($row as $j => $v) {
                $ref = $case['expected']['draws'][$i][$j];
                if ($ref != 0) check(abs($v / $ref - 1) < 1e-12, "$name relative output");
            }
            $back = Phit\draws_affine($result['draws'], $case['inverse'], $case['inverse_shift']);
            closeValues($back['draws'], $case['draws'], "$name round trip");
            closeValues($result['log_abs_det'] + $back['log_abs_det'], 0, "$name round trip logdet");
            break;
        default: throw new RuntimeException('Unknown fixture operation');
    }
}
$x = [[1, 2], [3, 4]];
$id = [[1, 0], [0, 1]];
$schema = json_decode(file_get_contents(__DIR__.'/fixtures/php-api.json'), true, flags: JSON_THROW_ON_ERROR);
foreach (['Phit\draws_moments'=>Phit\draws_moments($x),
          'Phit\draws_covariance_map'=>Phit\draws_covariance_map($id, $id),
          'Phit\draws_affine'=>Phit\draws_affine($x, $id, [0, 0])] as $name => $result) {
    $keys = array_keys($result); sort($keys);
    check($keys === $schema[$name]['result_keys'], "$name generated result contract");
}
closeValues(Phit\draws_moments($x), Phit\draws_moments($x, [1, 1], 'population'), 'defaults');
$subnormal = Phit\draws_affine([[1], [2]], [[1e-310]], [0]);
check($subnormal['draws'] === [[1e-310], [2e-310]], 'subnormal scale without reciprocal overflow');
closeValues($subnormal['log_abs_det'], log(1e-310), 'subnormal log determinant');
// Numeric arrays are consumed in iteration order and outputs are packed lists.
closeValues(Phit\draws_affine(['second'=>[9=>3, 2=>4], 'first'=>[1, 2]], $id, [0, 0])['draws'],
            [[3, 4], [1, 2]], 'draw order and literal columns');
foreach ([null, '1', true, []] as $bad) {
    code(fn() => Phit\draws_moments([[1, 2], [3, $bad]], [1, 0]), 1001);
    code(fn() => Phit\draws_moments($x, [1, $bad]), 1001);
    code(fn() => Phit\draws_covariance_map([[1, $bad], [0, 1]], $id), 1001);
    code(fn() => Phit\draws_covariance_map($id, [[1, 0], [$bad, 1]]), 1001);
    code(fn() => Phit\draws_affine([[1, $bad]], $id, [0, 0]), 1001);
    code(fn() => Phit\draws_affine($x, [[1, 0], [$bad, 1]], [0, 0]), 1001);
    code(fn() => Phit\draws_affine($x, $id, [0, $bad]), 1001);
}
foreach ([NAN, INF, -INF] as $bad) {
    code(fn() => Phit\draws_moments([[1, 2], [3, $bad]], [1, 0]), 1003);
    code(fn() => Phit\draws_moments($x, [1, $bad]), 1003);
    code(fn() => Phit\draws_covariance_map([[1, 0], [0, $bad]], $id), 1003);
    code(fn() => Phit\draws_covariance_map($id, [[1, 0], [0, $bad]]), 1003);
    code(fn() => Phit\draws_affine([[1, $bad]], $id, [0, 0]), 1003);
    code(fn() => Phit\draws_affine($x, [[1, 0], [0, $bad]], [0, 0]), 1003);
    code(fn() => Phit\draws_affine($x, $id, [0, $bad]), 1003);
}
foreach ([[], [[]]] as $bad) {
    code(fn() => Phit\draws_moments($bad), 1001);
    code(fn() => Phit\draws_covariance_map($bad, $id), 1001);
    code(fn() => Phit\draws_affine($bad, $id, [0, 0]), 1001);
}
code(fn() => Phit\draws_moments([1, 2]), 1001);
code(fn() => Phit\draws_moments([[1], [2, 3]]), 1002);
code(fn() => Phit\draws_moments($x, [1]), 1002);
code(fn() => Phit\draws_moments($x, [0, 0]), 1001);
code(fn() => Phit\draws_moments($x, [-1, 2]), 1001);
code(fn() => Phit\draws_moments($x, [1, 0], 'unbiased'), 1001);
code(fn() => Phit\draws_moments($x, [1, 1e-30], 'unbiased'), 1001);
code(fn() => Phit\draws_moments([[1]], normalization: 'unbiased'), 1001);
code(fn() => Phit\draws_moments($x, normalization: 'sample'), 1001);
code(fn() => Phit\draws_covariance_map([[1]], $id), 1002);
code(fn() => Phit\draws_covariance_map([[1, 0]], $id), 1002);
code(fn() => Phit\draws_covariance_map([[1, 0], [0]], $id), 1002);
code(fn() => Phit\draws_covariance_map([[1, 0.2], [0.3, 1]], $id), 1001);
foreach ([[[1, 1], [1, 1]], [[1, 2], [2, 1]], [[-1, 0], [0, 1]], [[0, 0], [0, 1]],
          [[1, 1-1e-15], [1-1e-15, 1]]] as $singular) {
    code(fn() => Phit\draws_covariance_map($singular, $id), 1005);
    code(fn() => Phit\draws_covariance_map($id, $singular), 1005);
}
check(is_array(Phit\draws_covariance_map([[1, 0.2+1e-15], [0.2, 1]], $id)), 'rounding-level asymmetry');
code(fn() => Phit\draws_covariance_map([[1, 0.2+1e-12], [0.2, 1]], $id), 1001);
code(fn() => Phit\draws_affine($x, [[1, 2], [2, 4]], [0, 0]), 1004);
code(fn() => Phit\draws_affine($x, [[1, 1], [1, 1+1e-15]], [0, 0]), 1004);
code(fn() => Phit\draws_affine($x, [[1, 2]], [0, 0]), 1002);
code(fn() => Phit\draws_affine($x, [[1, 0], [0]], [0, 0]), 1002);
code(fn() => Phit\draws_affine([[1], [2, 3]], $id, [0, 0]), 1002);
code(fn() => Phit\draws_affine($x, $id, [0]), 1002);
code(fn() => Phit\draws_affine([[PHP_FLOAT_MAX]], [[2]], [0]), 1003);
code(fn() => Phit\draws_moments([[-1e200], [1e200]]), 1003);
// No missing-row warnings, even on a failing call; singular moments remain valid.
set_error_handler(function($severity, $message) { throw new RuntimeException($message); });
code(fn() => Phit\draws_moments([[1], [null]]), 1001);
closeValues(Phit\draws_moments([[1, 2], [3, 4]], [1, 0]),
            ['mean'=>[1, 2], 'covariance'=>[[0, 0], [0, 0]]], 'one effective draw');
restore_error_handler();
echo count($fixture['cases'])." independent draw fixtures, identities and strict input contracts passed\n";
