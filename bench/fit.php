<?php
// One benchmark run in a fresh process: php bench/fit.php {ols|mixed} {data.csv|--baseline}
// With the extension loaded this times phit; without it, set PHIT_POLYFILL_AUTOLOAD to phit-polyfill's
// vendor/autoload.php. Prints one JSON line: elapsed ms, coefficients and standard errors. The runner measures memory.
declare(strict_types=1);

if (!extension_loaded('phit')) {
    require getenv('PHIT_POLYFILL_AUTOLOAD') ?: throw new RuntimeException('Load the extension or set PHIT_POLYFILL_AUTOLOAD');
}

use function Phit\{fit_mixed_gaussian, fit_ols};

[, $model, $path] = $argv;
if ($path === '--baseline') {
    exit;
}

// Untimed: read the CSV into PHP arrays of columns, the structure the fit functions take.
$file = fopen($path, 'r');
$names = fgetcsv($file, escape: '');
$data = array_fill_keys($names, []);
while (($row = fgetcsv($file, escape: '')) !== false) {
    foreach ($names as $j => $name) {
        $data[$name][] = $name === 'g' ? $row[$j] : (float) $row[$j];
    }
}
fclose($file);

$start = hrtime(true);
if ($model === 'ols') {
    $fit = fit_ols('y ~ x1 + x2 + x3 + x4 + x5 + x6 + x7 + x8 + x9 + x10', $data);
} else {
    $fit = fit_mixed_gaussian('y ~ x1 + x2 + x3 + (1 | g)', $data);
}
$coefficients = $fit['coefficients'];
$standardErrors = $fit['std_errors'];
$elapsed = (hrtime(true) - $start) / 1e6;

echo json_encode([
    'ms' => $elapsed, 'names' => $fit['columns'], 'coefficients' => $coefficients, 'std_errors' => $standardErrors,
    'version' => extension_loaded('phit') ? phpversion('phit') : null,
    'optimizer' => $fit['optimizer'] ?? null, 'evaluations' => $fit['evaluations'] ?? null,
]), "\n";
