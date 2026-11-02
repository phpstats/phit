<?php

declare(strict_types=1);

// Phit\summary() and Phit\tidy() against tests/fixtures/summary/summary.json: the summary text byte for byte, the
// tidy columns exactly, and each coefficient row's printed numbers against R's printCoefmat() for the same model.

function scheck(bool $condition, string $message): void
{
    if (! $condition) {
        throw new RuntimeException($message);
    }
}

// The same printed form, allowing one unit in the last printed place: phit and R stop their iterations at slightly
// different points, so a sixth significant digit can differ. The width and the number of decimals must match.
function sameToken(string $a, string $b): bool
{
    if ($a === $b) {
        return true;
    }
    $fixed = '/^-?\d+\.(\d+)$/';
    if (strlen($a) !== strlen($b) || ! preg_match($fixed, $a, $ma) || ! preg_match($fixed, $b, $mb)) {
        return false;
    }

    return strlen($ma[1]) === strlen($mb[1]) && abs((float) $a - (float) $b) <= 1.000001 * 10 ** -strlen($ma[1]);
}

$fixture = json_decode(file_get_contents(__DIR__.'/fixtures/summary/summary.json'), true, flags: JSON_THROW_ON_ERROR);
$rowsChecked = 0;
foreach ($fixture['cases'] as $case) {
    $args = $case['args'];
    if (isset($args['data'])) {
        $args['data'] = $fixture['data'][$args['data']];
    }
    $fit = ('Phit\\'.$case['function'])(...$args);
    $text = Phit\summary($fit);
    scheck($text === $case['summary'], "{$case['name']}: summary text differs:\n$text");
    scheck(Phit\tidy($fit, confInt: true) == $case['tidy'], "{$case['name']}: tidy differs");

    // The printed coefficient rows: every number R prints must print the same here.
    $lines = explode("\n", $text);
    $start = array_search(true, array_map(fn ($l) => in_array($l, ['Coefficients:', 'Fixed effects:'], true), $lines), true);
    foreach ($case['r_rows'] as $i => $rRow) {
        $ours = preg_split('/\s{1,}(?=[-<\d])/', trim($lines[$start + 2 + $i]));
        $theirs = preg_split('/\s{1,}(?=[-<\d])/', trim($rRow));
        $numbers = min(count($ours), count($theirs)) - 1;
        foreach (array_map(null, array_slice($ours, 1, $numbers), array_slice($theirs, 1, $numbers)) as [$a, $b]) {
            scheck(sameToken($a, $b), "{$case['name']}: row $i differs from R: '{$lines[$start + 2 + $i]}' vs '$rRow'");
        }
        $rowsChecked++;
    }
}

// Every model function's result has a printer: each one appears in a case above.
$covered = array_unique(array_column($fixture['cases'], 'function'));
foreach (get_extension_funcs('phit') as $function) {
    if (str_starts_with($function, 'Phit\\fit_')) {
        scheck(in_array(substr($function, 5), $covered, true), "no summary case for $function");
    }
}

$mixed = Phit\fit_mixed_gaussian(
    'Reaction ~ Days + (Days | Subject)', $fixture['data']['sleepstudy'],
);
$short = $mixed;
$short['random'][0]['covariance'] = [[1.0]];
$ragged = $mixed;
$ragged['random'][0]['covariance'][1] = [1.0];

foreach ([
    [$short, 'a covariance smaller than its columns'],
    [$ragged, 'a ragged covariance'],
    [[], 'not a fit'],
    [['model' => 'linear', 'coefficients' => [1.0]], 'missing a key'],
    [['model' => 'quantile', 'coefficients' => [1.0]], 'an unknown model'],
] as [$bad, $label]) {
    try {
        Phit\summary($bad);
        scheck(false, "summary accepted $label");
    } catch (Exception $e) {
        scheck($e->getCode() === 1001, "summary of $label: code {$e->getCode()}");
    }
}
try {
    Phit\tidy($fit, confInt: true, level: 1.0);
    scheck(false, 'tidy accepted level 1');
} catch (Exception $e) {
    scheck($e->getCode() === 1001 && $e->getMessage() === 'level must be in (0, 1)', 'tidy level error');
}

echo 'PHP summary and tidy tests passed ('.count($fixture['cases'])." models; $rowsChecked coefficient rows match R)\n";
