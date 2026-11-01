<?php

declare(strict_types=1);

// Usage: php csv.php MODE PATH REPEATS
//   MODE phit: Phit\read_csv() with guessed types.
//   MODE phibble: phibble's DataFrame::fromCsv() (PHIBBLE_AUTOLOAD names phibble's vendor/autoload.php). It uses
//   Phit\read_csv() when the extension is loaded, and its own PHP reader otherwise.
// Prints JSON: the seconds of each timed read after one warm-up, and the row count.

[, $mode, $path, $repeats] = $argv;
if ($mode === 'phibble') {
    require getenv('PHIBBLE_AUTOLOAD') ?: throw new RuntimeException('Set PHIBBLE_AUTOLOAD');
}
$read = match ($mode) {
    'phit' => fn () => count(Phit\read_csv($path)['columns'][0]['values']),
    'phibble' => fn () => Phibble\DataFrame::fromCsv($path)->nrow(),
};

$rows = $read();
$seconds = [];
for ($i = 0; $i < (int) $repeats; $i++) {
    $start = hrtime(true);
    $read();
    $seconds[] = (hrtime(true) - $start) / 1e9;
}
echo json_encode([
    'rows' => $rows,
    'seconds' => $seconds,
    'native' => function_exists('Phit\read_csv'),
    'php' => PHP_VERSION,
]), "\n";
