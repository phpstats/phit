<?php
// One benchmark run in a fresh process: php bench/fit.php WORKLOAD {data.csv|--baseline}
// With the extension loaded this times phit; without it, set PHIT_POLYFILL_AUTOLOAD to phit-polyfill's
// vendor/autoload.php. Prints one JSON line: elapsed ms and the estimates. The runner measures memory, and records
// the lines between a workload's [name] and [/name] markers as its timed code.
declare(strict_types=1);

if (!extension_loaded('phit')) {
    require getenv('PHIT_POLYFILL_AUTOLOAD') ?: throw new RuntimeException('Load the extension or set PHIT_POLYFILL_AUTOLOAD');
}

use function Phit\{dist_gamma_cdf_array, dist_gamma_quantile_array, dist_normal_cdf_array, dist_normal_quantile_array};
use function Phit\{fit_generalized_linear, fit_mixed_gaussian, fit_mixed_logistic, fit_ols};

[, $workload, $path] = $argv;
if ($path === '--baseline') {
    exit;
}

// Untimed: the data in PHP arrays, the structure the functions take.
if ($workload === 'distributions') {
    $n = 1_000_000;
    $x = $p = $positive = [];
    for ($i = 0; $i < $n; $i++) {
        $x[] = -5.0 + 10.0 * $i / ($n - 1);
        $p[] = ($i + 0.5) / $n;
        $positive[] = 20.0 * ($i + 0.5) / $n;
    }
} else {
    $file = fopen($path, 'r');
    $names = fgetcsv($file, escape: '');
    $data = array_fill_keys($names, []);
    while (($row = fgetcsv($file, escape: '')) !== false) {
        foreach ($names as $j => $name) {
            $data[$name][] = $name === 'g' ? $row[$j] : (float) $row[$j];
        }
    }
    fclose($file);
}

$start = hrtime(true);
if ($workload === 'ols') {
    // [ols]
    $fit = fit_ols('y ~ x1 + x2 + x3 + x4 + x5 + x6 + x7 + x8 + x9 + x10', $data);
    // [/ols]
} elseif ($workload === 'mixed') {
    // [mixed]
    $fit = fit_mixed_gaussian('y ~ x1 + x2 + x3 + (1 | g)', $data);
    // [/mixed]
} elseif ($workload === 'logistic') {
    // [logistic]
    $fit = fit_generalized_linear('y ~ x1 + x2 + x3 + x4 + x5 + x6 + x7 + x8 + x9 + x10', $data, family: 'binomial');
    // [/logistic]
} elseif ($workload === 'probit') {
    // [probit]
    $fit = fit_generalized_linear('y ~ x1 + x2 + x3 + x4 + x5 + x6 + x7 + x8 + x9 + x10', $data,
        family: 'binomial', link: 'probit');
    // [/probit]
} elseif ($workload === 'mixed-logistic') {
    // [mixed-logistic]
    $fit = fit_mixed_logistic('y ~ x1 + x2 + x3 + (1 | g)', $data);
    // [/mixed-logistic]
} else {
    // [distributions]
    $normalCdf = dist_normal_cdf_array($x);
    $normalQuantile = dist_normal_quantile_array($p);
    $gammaCdf = dist_gamma_cdf_array($positive, 2.5, 1.5);
    $gammaQuantile = dist_gamma_quantile_array($p, 2.5, 1.5);
    // [/distributions]
}
$elapsed = (hrtime(true) - $start) / 1e6;

$every = fn (array $v): array => array_values(array_filter($v, fn ($k) => $k % 1000 === 0, ARRAY_FILTER_USE_KEY));
echo json_encode($workload === 'distributions'
    ? ['ms' => $elapsed, 'values' => array_merge(...array_map($every, [$normalCdf, $normalQuantile, $gammaCdf, $gammaQuantile]))]
    : ['ms' => $elapsed, 'names' => $fit['columns'], 'coefficients' => $fit['coefficients'], 'std_errors' => $fit['std_errors'],
        'optimizer' => $fit['optimizer'] ?? null, 'evaluations' => $fit['evaluations'] ?? $fit['iterations'] ?? null]), "\n";
