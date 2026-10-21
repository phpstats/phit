<?php
// Conformance: Phit\read_csv() against the cases phibble generated from its own
// PHP reader, the reference implementation. tests/fixtures/csv-conformance.json
// is phibble's file, copied verbatim by bin/fetch-csv-fixtures from the phibble
// commit pinned in tests/fixtures/PHIBBLE_REFERENCE. No phibble checkout and no
// environment variable are needed, so scripts/check and CI run this every time.
//
// tests/csv-phibble.php is the other half: thousands of random cases compared
// against a phibble on disk, run by hand. This file is the always-on tier.
declare(strict_types=1);

// Floats are recorded as their shortest round-tripping decimal text, which is
// what json_encode() gives at the default serialize_precision of -1. The
// generator writes them the same way.
ini_set('serialize_precision', '-1');
if (json_encode(0.1) !== '0.1' || json_encode(1.0) !== '1' || json_encode(0.1 + 0.2) !== '0.30000000000000004') {
    throw new RuntimeException('serialize_precision does not give shortest round-tripping floats');
}

function check(bool $ok, string $message): void {
    if (!$ok) throw new RuntimeException($message);
}

/**
 * This file's one copy of the fixture encoding; phibble's is
 * csvConformanceScalar() in its bin/generate-csv-conformance. A string that is
 * not valid UTF-8 is recorded as {"b64": ...}, because JSON cannot hold the
 * raw bytes.
 */
function conformanceText(string $value): string|array {
    return preg_match('//u', $value) === 1 ? $value : ['b64' => base64_encode($value)];
}

/** Microseconds since the epoch as 2024-01-02T10:00:00.000000Z. */
function conformanceDateTime(int $microseconds): string {
    $seconds = intdiv($microseconds, 1000000);
    $fraction = $microseconds - $seconds * 1000000;
    if ($fraction < 0) { $seconds--; $fraction += 1000000; }
    return gmdate('Y-m-d\TH:i:s', $seconds).'.'.sprintf('%06d', $fraction).'Z';
}

/** One value as the fixture records it. Dates are days, datetimes microseconds. */
function conformanceScalar(mixed $value, string $type): mixed {
    if ($value === null) return null;
    return match ($type) {
        'date' => gmdate('Y-m-d', (int) $value * 86400),
        'datetime' => conformanceDateTime((int) $value),
        'float' => match (true) {
            is_nan($value) => 'NaN',
            is_infinite($value) => $value > 0 ? 'Inf' : '-Inf',
            default => (string) json_encode($value),
        },
        'string' => conformanceText((string) $value),
        default => $value,
    };
}

/** A Phit\read_csv() result in the fixture's encoding. */
function conformanceEncode(array $result): array {
    return [
        'names' => array_map(conformanceText(...), $result['names']),
        'columns' => array_map(fn (?array $c) => $c === null ? null : [
            'type' => $c['type'],
            'values' => array_map(fn (mixed $v) => conformanceScalar($v, $c['type']), $c['values']),
            'failed' => $c['failed'],
        ], $result['columns']),
        'problems' => array_map(conformanceText(...), $result['problems']),
    ];
}

/** A list of problems as a sorted list of JSON texts, so order does not matter. */
function problemSet(array $problems): array {
    $set = array_map(fn ($p) => json_encode($p), $problems);
    sort($set);
    return $set;
}

$file = __DIR__.'/fixtures/csv-conformance.json';
$document = json_decode((string) file_get_contents($file), true, 512, JSON_THROW_ON_ERROR);
check(isset($document['cases']) && $document['cases'] !== [], "$file holds no cases");

$dir = sys_get_temp_dir().'/phit-csv-conformance-'.getmypid();
@mkdir($dir);
$path = "$dir/case.csv";
$checked = 0;

foreach ($document['cases'] as $case) {
    $name = $case['name'];
    $input = array_key_exists('input_base64', $case)
        ? base64_decode($case['input_base64'], true)
        : $case['input'];
    check(is_string($input), "$name: the input is not a string");
    file_put_contents($path, $input);

    $actual = conformanceEncode(Phit\read_csv($path, $case['options']));
    $expected = $case['expected'];
    $show = fn (mixed $v) => var_export($v, true);

    check($actual['names'] === $expected['names'],
        "$name: names ".$show($actual['names']).' !== '.$show($expected['names']));
    check(count($actual['columns']) === count($expected['columns']),
        "$name: ".count($actual['columns']).' columns, expected '.count($expected['columns']));
    foreach ($expected['columns'] as $i => $want) {
        $got = $actual['columns'][$i];
        check($got === $want, "$name: column $i ".$show($got).' !== '.$show($want));
    }
    // "problems" is phibble's list; the native reader also reports unterminated
    // quoted fields, which the case names one by one. Equality against the
    // union, not a subtraction, so an unexpected extra problem fails here.
    $union = problemSet(array_merge($expected['problems'], $case['phit_extra_problems']));
    check(problemSet($actual['problems']) === $union,
        "$name: problems ".$show($actual['problems']).' !== '.$show(array_merge($expected['problems'], $case['phit_extra_problems'])));
    $checked++;
}

array_map(unlink(...), glob("$dir/*"));
rmdir($dir);
echo "csv-conformance: $checked cases matched phibble's reader\n";
