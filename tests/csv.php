<?php
// Phit\read_csv(): records, names, types, values and problems, following
// phibble's PHP reader (tests/csv-phibble.php compares the two directly).
declare(strict_types=1);

function check(bool $ok, string $message): void {
    if (!$ok) throw new RuntimeException($message);
}

$dir = sys_get_temp_dir().'/phit-csv-test-'.getmypid();
@mkdir($dir);
$checked = 0;

function csv(string $text, array $options = []): array {
    global $dir;
    $path = "$dir/input.csv";
    file_put_contents($path, $text);
    return Phit\read_csv($path, $options);
}

function fixture(string $name, array $options = []): array {
    return Phit\read_csv(__DIR__."/fixtures/csv/$name", $options);
}

/** Compare a column with ===, floats by bit pattern (NAN, -0.0). */
function column(array $result, int $i, string $type, array $values, int $failed = 0, string $label = ''): void {
    global $checked;
    $c = $result['columns'][$i];
    $label = $label === '' ? "column $i" : "$label column $i";
    check($c !== null, "$label is missing");
    check($c['type'] === $type, "$label type {$c['type']} !== $type");
    check(array_is_list($c['values']), "$label values must be a list");
    $canon = fn (array $v) => array_map(fn ($x) => is_float($x) ? 'f:'.bin2hex(pack('e', $x)) : $x, $v);
    check($canon($c['values']) === $canon($values), "$label values ".var_export($c['values'], true).' !== '.var_export($values, true));
    check($c['failed'] === $failed, "$label failed {$c['failed']} !== $failed");
    $checked++;
}

function same(mixed $actual, mixed $expected, string $label): void {
    global $checked;
    check($actual === $expected, "$label: ".var_export($actual, true).' !== '.var_export($expected, true));
    $checked++;
}

function valueError(callable $f, string $contains): void {
    global $checked;
    try {
        $f();
    } catch (ValueError $e) {
        check(str_contains($e->getMessage(), $contains), "message '{$e->getMessage()}' lacks '$contains'");
        $checked++;
        return;
    }
    throw new RuntimeException("expected ValueError containing '$contains'");
}

// --- Shape -------------------------------------------------------------
$r = fixture('quoting.csv');
same(array_keys($r), ['names', 'columns', 'problems'], 'result keys');
same(array_keys($r['columns'][0]), ['type', 'values', 'failed'], 'column keys');

// --- Quoting -------------------------------------------------------------
same($r['names'], ['id', 'text', 'note'], 'quoting names');
column($r, 0, 'int', [1, 2, 3, 4]);
// Delimiters, doubled quotes and newlines inside quotes; text after a closing
// quote is kept; "" is the empty string, which is NA.
column($r, 1, 'string', ['a,b', "multi\nline", 'plain', null]);
column($r, 2, 'string', ['say "hi"', 'padded', 'x"y', null]);
same($r['problems'], [], 'quoting problems');

$r = csv("a,b\n\"x\"y,\" sp \"\n");
column($r, 0, 'string', ['xy']);
column($r, 1, 'string', ['sp'], label: 'trimmed after unquoting');
$r = csv("a\n'x,y'\n", ['quote' => "'"]);
column($r, 0, 'string', ['x,y'], label: 'single quote');
$r = csv("a,b\n\"x\",1\n", ['quote' => '']);
column($r, 0, 'string', ['"x"'], label: 'no quote');

// --- Escapes -------------------------------------------------------------
$r = csv("a,b\n\"say \\\"hi\\\"\",2\n", ['escape_backslash' => true, 'escape_double' => false]);
column($r, 0, 'string', ['say "hi"'], label: 'backslash');
$r = csv("a\n\"x\"\"y\"\n", ['escape_double' => false]);
column($r, 0, 'string', ['x"y"'], label: 'no double escape');
$r = csv("a\nx\\,y\n", ['escape_backslash' => true]);
column($r, 0, 'string', ['x,y'], label: 'unquoted backslash');

// --- NA --------------------------------------------------------------------
$r = csv("a,b\n\"NA\",1\n NA ,2\nx,\n");
column($r, 0, 'string', [null, null, 'x'], label: 'quoted and padded NA');
column($r, 1, 'int', [1, 2, null]);
$r = csv("a,b\n-,1\nmissing,\nNA,3\n", ['na' => ['-', 'missing']]);
column($r, 0, 'string', [null, null, 'NA'], label: 'custom na');
column($r, 1, 'string', ['1', '', '3'], label: 'empty is not NA');
$r = csv("a\nNA\n\n", ['skip_empty_rows' => false]);
column($r, 0, 'bool', [null, null], label: 'all NA');

// --- Whitespace ------------------------------------------------------------
$r = csv(" a ,\tb\n 1 ,\tx\t\n");
same($r['names'], ['a', 'b'], 'trimmed names');
column($r, 0, 'int', [1]);
column($r, 1, 'string', ['x']);
$r = csv(" a ,b\n 1 ,x \n", ['trim_ws' => false]);
same($r['names'], [' a ', 'b'], 'untrimmed names');
column($r, 0, 'string', [' 1 '], label: 'untrimmed');
column($r, 1, 'string', ['x ']);

// --- skip, n_max, comments, empty rows -------------------------------------
$r = csv("junk\njunk,2\na,b\n1,2\n3,4\n5,6\n", ['skip' => 2, 'n_max' => 2]);
same($r['names'], ['a', 'b'], 'skip names');
column($r, 0, 'int', [1, 3], label: 'skip/n_max');
same(csv("a,b\n1,2\n", ['n_max' => 0])['names'], ['a', 'b'], 'header probe');
$r = csv("a\n1\n", ['skip' => 5]);
same([$r['names'], $r['columns']], [[], []], 'skip past the end');
$r = fixture('comment.csv', ['comment' => '#']);
same($r['names'], ['x', 'y'], 'comment names');
column($r, 0, 'int', [1, 3], label: 'comment');
column($r, 1, 'int', [2, 4], label: 'trailing comment');
// A kept empty row is one empty field, as readr reads it, not a row of no
// fields, so it is one column short rather than three.
$r = fixture('ragged.csv', ['skip_empty_rows' => false]);
column($r, 0, 'int', [1, 4, 6, null, 10], label: 'kept empty row');
same($r['problems'], ['row 3: expected 3 columns, got 2', 'row 4: expected 3 columns, got 4',
    'row 5: expected 3 columns, got 1'], 'empty row problem');

// --- No data rows ------------------------------------------------------------
// As phibble does: with no data rows a column is string, before the guesser's
// empty-column rule can make it bool. Four ways to read no data rows.
$r = csv("a,b\n");
same($r['names'], ['a', 'b'], 'header-only names');
column($r, 0, 'string', [], label: 'header-only');
column($r, 1, 'string', [], label: 'header-only');
column(csv("a,b\n1,2\n", ['n_max' => 0]), 0, 'string', [], label: 'n_max 0');
column(csv("junk\njunk\na,b\n", ['skip' => 2]), 0, 'string', [], label: 'every row skipped');
column(csv("a,b\n\n\n"), 0, 'string', [], label: 'every row empty');
// An explicit type code still wins, and one data row is enough for the guesser's
// empty-column rule to apply again.
column(csv("a,b\n", ['types' => ['l']]), 0, 'bool', [], label: 'header-only, typed');
column(csv("a\nNA\n"), 0, 'bool', [null], label: 'all NA with a data row');
column(csv("a,b\n\n\n", ['skip_empty_rows' => false]), 0, 'bool', [null, null], label: 'empty rows kept');

// --- col_names ---------------------------------------------------------------
$r = csv("1,x\n2,y\n", ['col_names' => false]);
same($r['names'], ['X1', 'X2'], 'generated names');
column($r, 0, 'int', [1, 2]);
column($r, 1, 'string', ['x', 'y']);

// --- Ragged rows --------------------------------------------------------------
$r = fixture('ragged.csv');
same($r['names'], ['a', 'b', 'c'], 'ragged names');
column($r, 0, 'int', [1, 4, 6, 10]);
column($r, 1, 'int', [2, 5, 7, 11]);
// The extra field joins the last column: "8,9" then guesses as a grouped int.
column($r, 2, 'int', [3, null, 89, 12], label: 'ragged');
same($r['problems'], ['row 3: expected 3 columns, got 2', 'row 4: expected 3 columns, got 4'], 'ragged problems');
$r = csv("a,b\n1,x,y,z\n");
column($r, 1, 'string', ['x,y,z'], label: 'joined extras');
$r = csv("a;b\n1;x;y\n", ['delimiter' => ';']);
column($r, 1, 'string', ['x;y'], label: 'joined with the delimiter');

// --- BOM, line endings, unterminated quotes, empty input ----------------------
same(fixture('bom.csv')['names'], ['name', 'n'], 'BOM');
$r = fixture('crlf.csv');
column($r, 0, 'int', [1, 2], label: 'CRLF');
column($r, 1, 'string', ['x', 'y'], label: 'CRLF');
$r = csv("a,b\r\n\"x\r\ny\",1\r\n");
column($r, 0, 'string', ["x\r\ny"], label: 'CRLF inside quotes');
// Text without LF reads CR as the line ending; with LF, a lone CR is data.
$r = csv("a,b\r1,2\r3,4\r");
same($r['names'], ['a', 'b'], 'CR-only names');
column($r, 0, 'int', [1, 3], label: 'CR-only');
column($r, 1, 'int', [2, 4], label: 'CR-only');
$r = csv("a,b\r1,2\r", ['comment' => '#']);
column($r, 0, 'int', [1], label: 'CR-only, character reader');
$r = csv("a\nx\ry\n");
column($r, 0, 'string', ["x\ry"], label: 'lone CR with LF line endings');
// The byte order mark goes before tokenizing, so a quoted header loses its quotes.
same(csv("\xEF\xBB\xBF\"id\",\"x\"\n1,2\n")['names'], ['id', 'x'], 'BOM before a quoted header');
same(csv("\xEF\xBB\xBF\"id\",\"x\"\n1,2\n", ['comment' => '#'])['names'], ['id', 'x'], 'BOM, character reader');
same(csv("skipped\n\xEF\xBB\xBFid,x\n1,2\n", ['skip' => 1])['names'], ['id', 'x'], 'BOM after skipped lines');
$r = csv("a,b\n1,\"open\n");
column($r, 1, 'string', ["open\n"], label: 'unterminated');
same($r['problems'], ['row 2: unterminated quoted field at end of file'], 'unterminated problem');
$r = csv("a,b\n1,\"open");
column($r, 1, 'string', ['open'], label: 'unterminated without newline');
foreach (['', "\n\n", "\r\n"] as $empty) {
    same(csv($empty), ['names' => [], 'columns' => [], 'problems' => []], 'empty input '.json_encode($empty));
}
$r = csv("a,b\n\xff\xfe,\xC3\xA9\n");
column($r, 0, 'string', ["\xff\xfe"], label: 'raw bytes');

// --- Guessing ------------------------------------------------------------------
$r = fixture('guess.csv');
same($r['names'], ['b', 'i', 'i0', 'g', 'd', 'd0', 'inf', 'dt', 'dtm', 's'], 'guess names');
column($r, 0, 'bool', [true, false, null]);
column($r, 1, 'int', [1, -7, null]);
column($r, 2, 'int', [0, 0, null]);
column($r, 3, 'int', [1234, 12, 1000000], label: 'grouping');
column($r, 4, 'float', [1.5, 100000.0, 0.5]);
column($r, 5, 'float', [0.5, -0.5, null]);
column($r, 6, 'float', [INF, NAN, null]);
column($r, 7, 'date', [18263, 18627, null]);
column($r, 8, 'datetime', [1577934245000000, 1577914445250000, 1577923200000000], label: 'offsets');
column($r, 9, 'string', ['007', 'x', null], label: 'leading zero');

$guess = fn (array $values, array $options = []) => csv("v\n".implode("\n", $values)."\n", $options)['columns'][0];
$cases = [
    [['T', 'F', 'TRUE', 'FALSE', 'True', 'False', 'true', 'false', 't', 'f'], 'bool'],
    [['1', '0'], 'int'], [['yes'], 'string'],
    [['007'], 'string'], [['0'], 'int'], [['-07'], 'int'], [['+5'], 'int'], [['-0'], 'int'],
    [['"1,234"'], 'int'], [['"1,,2"'], 'int'], [['"1,"'], 'float'],
    [['9223372036854775807'], 'int'], [['-9223372036854775808'], 'int'], [['9223372036854775808'], 'float'],
    [['1.5'], 'float'], [['1.'], 'float'], [['.5'], 'float'], [['1e5'], 'float'], [['-1E-3'], 'float'],
    [['0e5'], 'string'], [['00.5'], 'string'], [['0.5'], 'float'], [['-00.5'], 'float'], [['.'], 'string'],
    [['Inf'], 'float'], [['-inf'], 'float'], [['NaN'], 'float'], [['infinity'], 'string'],
    [['2020-01-02'], 'date'], [['2020/01/02'], 'date'], [['2020-01/02'], 'date'], [['2020-02-30'], 'date'],
    [['2020-1-2'], 'string'], [['2020-01-02', '2020-01-02 03:04'], 'datetime'], [['2020-01-02T03'], 'datetime'],
    [['2020-01-02T03:04:05.5Z'], 'datetime'], [['2020-01-02+0530'], 'datetime'], [['2020/01/02 03:04'], 'string'],
    [['1', '1.5'], 'float'], [['1', 'x'], 'string'], [['T', '1'], 'string'],
];
foreach ($cases as [$values, $type]) {
    same($guess($values)['type'], $type, 'guess '.implode(' ', $values));
}
// An invalid calendar date still guesses date (the pattern decides) and then fails to parse.
same($guess(['2020-02-30', '2020-03-01']), ['type' => 'date', 'values' => [null, 18322], 'failed' => 1], 'invalid date');
same($guess(['"1,234"'], ['grouping_mark' => ''])['type'], 'string', 'no grouping mark');
same($guess(['"1 234"'], ['grouping_mark' => ' '])['values'], [1234], 'space grouping');
same($guess(['"1,5"', '"1.234,5"'], ['decimal_mark' => ','])['values'], [1.5, 1234.5], 'decimal comma');

// Sampling: 3 rows with guess_max 2 look at rows 0 and 2 only.
$r = csv("x\n1\na\n3\n", ['guess_max' => 2]);
column($r, 0, 'int', [1, null, 3], 1, 'sampled');
$r = csv("x\n1\na\n3\n", ['guess_max' => 3]);
column($r, 0, 'string', ['1', 'a', '3'], label: 'all rows sampled');
$lines = ['x'];
for ($i = 0; $i < 2000; $i++) $lines[] = $i === 1001 ? 'oops' : (string) $i;
column(csv(implode("\n", $lines)), 0, 'int', array_map(fn ($i) => $i === 1001 ? null : $i, range(0, 1999)), 1, 'default guess_max');

// --- Datetimes ---------------------------------------------------------------
$dt = fn (string $v) => csv("t\n$v\n", ['types' => ['T']])['columns'][0]['values'][0];
same($dt('1970-01-01'), 0, 'epoch');
same($dt('1969-12-31T23:59:59.5'), -500000, 'before the epoch');
same($dt('2020-01-02T03:04:05.123456789Z'), 1577934245123456, 'microseconds truncated');
same($dt('2020-01-02T03:04:05-0800'), 1577934245000000 + 8 * 3600 * 1000000, 'offset without colon');
same($dt('2020-01-02T03:04:05-08'), 1577934245000000 + 8 * 3600 * 1000000, 'hour offset');
same($dt('2020-01-02 03:04:05+24:59'), 1577934245000000 - (24 * 3600 + 59 * 60) * 1000000, 'largest offset');
same($dt('2020-01-02 03:04:05+25:00'), null, 'offset out of range');
same($dt('2020-01-02 24:00:00'), null, 'hour 24');
same($dt('2020-01-02 03:60'), null, 'minute 60');
same($dt('2020/01/02 03:04'), null, 'slashes in a datetime');

// --- Explicit types ------------------------------------------------------------
$r = csv("l,i,d,c,D,T,_\n1,007,1.,x,2020-01-02,2020-01-02 03:04:05,zz\nyes,\"1,234\",abc,y,2020-02-30,bad,zz\nT,-5,\"1,5\",,2020/01/03,2020-01-02,\n",
    ['types' => ['l', 'i', 'd', 'c', 'D', 'T', '_']]);
column($r, 0, 'bool', [true, null, true], 1);
column($r, 1, 'int', [7, null, -5], 1, 'typed int (leading zeros, no grouping)');
column($r, 2, 'float', [1.0, null, null], 2, 'typed double');
column($r, 3, 'string', ['x', 'y', null]);
column($r, 4, 'date', [18263, null, 18264], 1);
column($r, 5, 'datetime', [1577934245000000, null, 1577923200000000], 1);
same($r['columns'][6], null, 'skipped column');
$r = csv("a,b,c\n1,2,3\n", ['types' => [1 => 'c']]);
same(array_map(fn ($c) => $c['type'], $r['columns']), ['int', 'string', 'int'], 'sparse types');
$r = csv("a\n1\n", ['types' => [0 => '?', 5 => 'c']]);
column($r, 0, 'int', [1], label: 'positions past the end');

// --- Errors ------------------------------------------------------------------------
valueError(fn () => Phit\read_csv("$dir/missing.csv"), 'cannot read CSV file');
valueError(fn () => Phit\read_csv($dir), 'cannot read CSV file');
valueError(fn () => csv("a\n", ['delimiter' => '::']), "'delimiter' must be a single byte");
valueError(fn () => csv("a\n", ['delimiter' => '']), "'delimiter' must be a single byte");
valueError(fn () => csv("a\n", ['quote' => '""']), "'quote' must be an empty string or a single byte");
valueError(fn () => csv("a\n", ['types' => ['x']]), "unknown type code 'x'");
valueError(fn () => csv("a\n", ['types' => ['f']]), "unknown type code 'f'");
valueError(fn () => csv("a\n", ['types' => ['a' => 'c']]), '0-based column positions');
valueError(fn () => csv("a\n", ['delimter' => ',']), "unknown option 'delimter'");
valueError(fn () => csv("a\n", ['skip' => -1]), "'skip' must be an int");
valueError(fn () => csv("a\n", ['guess_max' => 0]), "'guess_max' must be an int of at least 1");
valueError(fn () => csv("a\n", ['trim_ws' => 1]), "'trim_ws' must be a bool");
valueError(fn () => csv("a\n", ['na' => 'NA']), "'na' must be a list of strings");
valueError(fn () => csv("a\n", ['decimal_mark' => ',', 'grouping_mark' => ',']), 'must differ');
valueError(fn () => csv("a\n", ['grouping_mark' => '1']), 'grouping_mark must not be a digit');
$r = csv("a\n1\n", ['delimiter' => ',', 'quote' => '"', 'escape_double' => true, 'escape_backslash' => false, 'na' => ['', 'NA'],
    'trim_ws' => true, 'skip' => 0, 'n_max' => null, 'comment' => '', 'skip_empty_rows' => true, 'col_names' => true,
    'types' => [], 'guess_max' => 1000, 'decimal_mark' => '.', 'grouping_mark' => null]);
column($r, 0, 'int', [1], label: 'every option at its default');

array_map(unlink(...), glob("$dir/*"));
rmdir($dir);
echo "csv: $checked checks passed\n";
