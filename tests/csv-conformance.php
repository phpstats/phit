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

/** Chunks of $rows rows from Phit\CsvReader, checking their positions. */
function chunks(string $path, array $options, int $rows): array {
    $reader = new Phit\CsvReader($path, $options);
    $out = [];
    $next = 1;
    while (($chunk = $reader->next($rows)) !== null) {
        check($chunk['position'] === $next, "chunk at {$chunk['position']}, expected $next");
        check($chunk['names'] === $reader->names(), 'chunk names');
        $next += $chunk['rows'];
        $out[] = $chunk;
    }
    return $out;
}

foreach ($document['cases'] as $case) {
    $name = $case['name'];
    $input = array_key_exists('input_base64', $case)
        ? base64_decode($case['input_base64'], true)
        : $case['input'];
    check(is_string($input), "$name: the input is not a string");
    file_put_contents($path, $input);
    $expected = $case['expected'];
    $show = fn (mixed $v) => var_export($v, true);
    $compare = function (array $result, string $label) use ($expected, $case, $show) {
        $actual = conformanceEncode($result);
        check($actual['names'] === $expected['names'],
            "$label: names ".$show($actual['names']).' !== '.$show($expected['names']));
        check(count($actual['columns']) === count($expected['columns']),
            "$label: ".count($actual['columns']).' columns, expected '.count($expected['columns']));
        foreach ($expected['columns'] as $i => $want) {
            $got = $actual['columns'][$i];
            check($got === $want, "$label: column $i ".$show($got).' !== '.$show($want));
        }
        // "problems" is phibble's list; the native reader also reports unterminated
        // quoted fields, which the case names one by one. Equality against the
        // union, not a subtraction, so an unexpected extra problem fails here.
        $union = problemSet(array_merge($expected['problems'], $case['phit_extra_problems']));
        check(problemSet($actual['problems']) === $union,
            "$label: problems ".$show($actual['problems']).' !== '.$show(array_merge($expected['problems'], $case['phit_extra_problems'])));
    };

    $whole = Phit\read_csv($path, $case['options']);
    $compare($whole, $name);
    // The result does not depend on where the input's blocks end.
    foreach ([1, 2, 3, 5, 8, 64] as $blockSize) {
        $compare(Phit\read_csv($path, ['block_size' => $blockSize] + $case['options']), "$name, $blockSize-byte blocks");
    }
    // One chunk holding every row is the whole read.
    $all = chunks($path, ['block_size' => 3] + $case['options'], PHP_INT_MAX);
    if ($all === []) {
        check($whole['problems'] === [] && array_column(array_filter($whole['columns']), 'values') === array_fill(0, count(array_filter($whole['columns'])), []),
            "$name: no chunks, but read_csv() has rows or problems");
    } else {
        check(count($all) === 1, "$name: one chunk expected");
        $compare(['names' => $all[0]['names'], 'columns' => $all[0]['columns'], 'problems' => $all[0]['problems']], "$name, one chunk");
    }
    // Small chunks: problems in order, and as text the same rows.
    $text = $case['options'];
    $width = count($whole['names']);
    $text['types'] = [];
    for ($i = 0; $i < $width; $i++) {
        $text['types'][$i] = ($case['options']['types'][$i] ?? '?') === '_' ? '_' : 'c';
    }
    $strings = Phit\read_csv($path, $text);
    foreach ([1, 2, 3] as $rows) {
        foreach ([1, 1 << 22] as $blockSize) {
            $label = "$name, $rows-row chunks, $blockSize-byte blocks";
            $got = chunks($path, ['block_size' => $blockSize] + $case['options'], $rows);
            check(array_merge([], ...array_column($got, 'problems')) === $whole['problems'], "$label: problems");
            $got = chunks($path, ['block_size' => $blockSize] + $text, $rows);
            foreach ($strings['columns'] as $i => $column) {
                $values = array_merge([], ...array_map(fn ($c) => $c['columns'][$i]['values'] ?? [], $got));
                check($column === null ? array_column($got, 'columns') === [] || $got[0]['columns'][$i] === null
                    : $values === $column['values'], "$label: column $i");
            }
        }
    }
    $checked++;
}

array_map(unlink(...), glob("$dir/*"));
rmdir($dir);
echo "csv-conformance: $checked cases matched phibble's reader, at every block size and in chunks\n";
