<?php
// Differential test: Phit\read_csv() against phibble's PHP CSV reader (the
// reference implementation). Runs only when PHIBBLE_AUTOLOAD points at
// phibble's vendor/autoload.php; otherwise it is skipped.
declare(strict_types=1);

$autoload = getenv('PHIBBLE_AUTOLOAD');
if ($autoload === false || $autoload === '') {
    echo "csv-phibble: skipped (set PHIBBLE_AUTOLOAD to phibble's vendor/autoload.php)\n";
    return;
}
require $autoload;

use Phibble\Column\TemporalColumn;
use Phibble\DataFrame;
use Phibble\Io\NativeCsv;

function check(bool $ok, string $message): void {
    if (!$ok) throw new RuntimeException($message);
}

/** A comparable form of a value: floats by bit pattern, so NAN and -0.0 compare exactly. */
function canonical(mixed $v): mixed {
    return is_float($v) ? 'f:'.bin2hex(pack('e', $v)) : $v;
}

/** @return array{0: ?array, 1: list<string>, 2: ?string} names/types/values, warnings, error */
function readWith(bool $native, string $path, array $options): array {
    NativeCsv::enable($native);
    $warnings = [];
    set_error_handler(function (int $level, string $message) use (&$warnings) {
        if ($level === E_USER_WARNING) $warnings[] = $message;
        return true;
    });
    try {
        $df = DataFrame::fromCsv($path, ...$options);
    } catch (Throwable $e) {
        return [null, $warnings, get_class($e).': '.$e->getMessage()];
    } finally {
        restore_error_handler();
        NativeCsv::enable(true);
    }
    $out = [];
    foreach ($df->columns() as $name) {
        $storage = $df->storage($name);
        $values = $storage instanceof TemporalColumn ? $storage->ticks() : $storage->values();
        $out[] = [$name, $df->type($name)->value, array_map(canonical(...), $values),
            $df->type($name)->value === 'factor' ? $df->levels($name) : null];
    }
    return [$out, $warnings, null];
}

function compareReaders(string $text, array $options, string $label, string $dir): void {
    static $n = 0;
    $path = $dir.'/case'.($n++ % 50).'.csv';
    file_put_contents($path, $text);
    [$php, $phpWarnings, $phpError] = readWith(false, $path, $options);
    [$rust, $rustWarnings, $rustError] = readWith(true, $path, $options);
    $context = $label.' '.json_encode(['text' => $text, 'options' => $options], JSON_INVALID_UTF8_SUBSTITUTE);
    if ($phpError !== null) {
        check($rustError !== null, "PHP failed ($phpError) but native did not: $context");
        return;
    }
    check($rustError === null, "native failed ($rustError): $context");
    check($php === $rust, "results differ: $context\nphp: ".var_export($php, true)."\nnative: ".var_export($rust, true));
    check($phpWarnings === $rustWarnings, "warnings differ: $context\n".var_export([$phpWarnings, $rustWarnings], true));
}

function nativeOptions(array $o): array {
    $map = ['delimiter' => 'delimiter', 'quote' => 'quote', 'escapeDouble' => 'escape_double',
        'escapeBackslash' => 'escape_backslash', 'na' => 'na', 'trimWs' => 'trim_ws', 'skip' => 'skip',
        'nMax' => 'n_max', 'comment' => 'comment', 'skipEmptyRows' => 'skip_empty_rows',
        'colNames' => 'col_names', 'guessMax' => 'guess_max', 'decimalMark' => 'decimal_mark', 'groupingMark' => 'grouping_mark'];
    $out = [];
    foreach ($o as $k => $v) if (isset($map[$k])) $out[$map[$k]] = $v;
    return $out;
}

$dir = sys_get_temp_dir().'/phit-csv-'.getmypid();
@mkdir($dir);
$compared = 0;

// 1. The csv.php fixtures through both readers with default options.
foreach (glob(__DIR__.'/fixtures/csv/*.csv') as $file) {
    compareReaders((string) file_get_contents($file), [], basename($file), $dir);
    $compared++;
}

// 2. Random inputs built from tokens that exercise quoting, escapes, line
// endings, whitespace, comments and every value grammar.
$tokens = ['a', 'b c', '1', '0', '-0', '+5', '007', '-07', '1,234', '1,,2', '1,', '12345678901234567890', '-9223372036854775808',
    '1.5', '1.', '.5', '-.5e-3', '0.5', '00.5', '0e5', '1e5', '1e+05', '1E400', '1,5', '1.234,5', 'Inf', '-inf', 'NaN', 'nan',
    'T', 'F', 'true', 'FALSE', 'x', 'NA', '', ' ', "\t", '  1 ', 'na', 'TRUE ',
    '2020-01-02', '2020/01/02', '2020-01/02', '2020-02-30', '0000-01-01', '2020-13-01', '1969-12-31',
    '2020-01-02 03:04:05', '2020-01-02T03:04', '2020-01-02T03', '2020-01-02T03:04:05.123456789Z', '2020-01-02T03:04:05+05:30',
    '2020-01-02T03:04:05-0800', '2020-01-02 03:04:05+24:59', '2020-01-02 03:04:05+25:00', '2020-01-02Z', '2020-01-02 24:00:00',
    '1969-12-31T23:59:59.5', '2020-01-02 03:04:60',
    '"', '""', '"q"', '"a,b"', '"a""b"', '"x', 'x"y', '"a" b', ' "a"', '"x "y" z"', '"x "y",z"', '"a"x""', '  "a,b"', '"x \"y\" z"', '"""', "\"multi\nline\"", "\"cr\r\nlf\"", '\\', '\\"', '\\,',
    '#', '# note', "'", "'s'", ';', "\xEF\xBB\xBF", "\xC3\xA9", "\0", "\v", "\r"];
$separators = [',', ',', ',', ';', "\t", ' '];
$ends = ["\n", "\n", "\n", "\r\n", "\r", ''];
mt_srand((int) (getenv('CSV_FUZZ_SEED') ?: 20261016));
$cases = (int) (getenv('CSV_FUZZ_CASES') ?: 3000);
for ($case = 0; $case < $cases; $case++) {
    $delimiter = $separators[mt_rand(0, count($separators) - 1)];
    $rows = [];
    $nrows = mt_rand(0, 7);
    $width = mt_rand(1, 4);
    $end = $ends[mt_rand(0, count($ends) - 1)];
    for ($r = 0; $r < $nrows; $r++) {
        $w = mt_rand(0, 9) === 0 ? mt_rand(0, 6) : $width;
        $fields = [];
        for ($f = 0; $f < $w; $f++) {
            $fields[] = mt_rand(0, 3) === 0 ? $tokens[mt_rand(0, count($tokens) - 1)].$tokens[mt_rand(0, count($tokens) - 1)]
                : $tokens[mt_rand(0, count($tokens) - 1)];
        }
        $rows[] = implode(mt_rand(0, 12) === 0 ? ',' : $delimiter, $fields);
    }
    $text = implode($end, $rows).(mt_rand(0, 2) > 0 ? $end : '');
    if (mt_rand(0, 9) === 0) $text = "\xEF\xBB\xBF".$text;
    $options = ['delimiter' => $delimiter];
    if (mt_rand(0, 3) === 0) $options['quote'] = ['', "'", '"'][mt_rand(0, 2)];
    if (mt_rand(0, 4) === 0) $options['escapeDouble'] = false;
    if (mt_rand(0, 4) === 0) $options['escapeBackslash'] = true;
    if (mt_rand(0, 3) === 0) $options['comment'] = ['#', '# ', '//'][mt_rand(0, 2)];
    if (mt_rand(0, 4) === 0) $options['trimWs'] = false;
    if (mt_rand(0, 4) === 0) $options['skip'] = mt_rand(0, 3);
    if (mt_rand(0, 4) === 0) $options['nMax'] = mt_rand(0, 4);
    if (mt_rand(0, 4) === 0) $options['skipEmptyRows'] = false;
    if (mt_rand(0, 4) === 0) $options['colNames'] = false;
    if (mt_rand(0, 3) === 0) $options['guessMax'] = mt_rand(1, 4);
    if (mt_rand(0, 4) === 0) $options['na'] = [['NA'], ['', 'NA', 'na', '-'], [], [' ']][mt_rand(0, 3)];
    if (mt_rand(0, 5) === 0) $options['decimalMark'] = ',';
    if (mt_rand(0, 5) === 0) $options['groupingMark'] = ['', ' ', "'"][mt_rand(0, 2)];
    compareReaders($text, $options, "random case $case", $dir);
    $compared++;
    // The same text with explicit types.
    [$columns] = readWith(false, $dir.'/case'.(($compared - 1) % 50).'.csv', $options + ['colNames' => $options['colNames'] ?? true]);
    if ($columns !== null && count($columns) > 0 && mt_rand(0, 1) === 0) {
        $letters = 'lidcDT_?fn';
        $types = '';
        for ($i = 0; $i < count($columns); $i++) $types .= $letters[mt_rand(0, strlen($letters) - 1)];
        compareReaders($text, $options + ['types' => $types], "typed case $case", $dir);
        $compared++;
    }
}

// 3. Sampling: more rows than guess_max, with the deciding value off and on the sampled rows.
for ($n = 1; $n <= 40; $n++) {
    foreach ([1, 2, 3, 7, 10] as $guessMax) {
        $lines = ['x,y'];
        for ($i = 0; $i < $n; $i++) $lines[] = ($i % 5 === 3 ? 'a' : (string) $i).','.($i === $n - 1 ? '1.5' : '2');
        compareReaders(implode("\n", $lines)."\n", ['guessMax' => $guessMax], "sampling $n/$guessMax", $dir);
        $compared++;
    }
}

array_map(unlink(...), glob("$dir/*"));
rmdir($dir);
echo "csv-phibble: $compared comparisons against phibble's PHP reader passed\n";
