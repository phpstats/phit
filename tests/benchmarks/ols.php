<?php
// One PHP worker. Native Phit\fit_ols_matrix returns its complete public result each time.
$start = hrtime(true);
$stream = fopen($argv[1], 'r');
fgetcsv($stream, escape: '');
$x = $y = $groups = [];
while (($row = fgetcsv($stream, escape: '')) !== false) {
    $numeric = array_map('floatval', $row);
    $y[] = array_shift($numeric);
    if (($argv[3] ?? 'ols') === 'lmm') $groups[] = (int) array_shift($numeric);
    $x[] = $numeric;
}
fclose($stream);
$loadSeconds = (hrtime(true) - $start) / 1e9;
$model = $argv[3] ?? 'ols';
if (!in_array($model, ['ols','logistic','probit','poisson','lmm'], true)) throw new InvalidArgumentException('Unknown model');
$function = 'Phit\\' . ['ols' => 'fit_ols_matrix', 'logistic' => 'fit_logistic_matrix', 'probit' => 'fit_probit_matrix', 'poisson' => 'fit_poisson_matrix', 'lmm' => 'fit_mixed_gaussian_intercept_matrix'][$model];
$indices = [];
for ($i = 0; $i < 10; $i++) $indices[] = (int) floor($i * (count($y) - 1) / 9);
$indices = array_values(array_unique($indices));
$warmup = ($model === 'lmm' ? $function($x, $y, $groups, intercept: false) : $function($x, $y, intercept: false));
unset($warmup);
$times = [];
for ($i = 0; $i < (int) $argv[2]; $i++) {
    $start = hrtime(true);
    $result = ($model === 'lmm' ? $function($x, $y, $groups, intercept: false) : $function($x, $y, intercept: false));
    $times[] = (hrtime(true) - $start) / 1e9;
    // Compact verification/output happens outside the timed call.
    $compact = $result;
    $compact['fitted_sample'] = array_map(fn($j) => $result['fitted'][$j], $indices);
    $compact['residuals_sample'] = array_map(fn($j) => $result['residuals'][$j], $indices);
    if ($model === 'lmm') {
        foreach (['fixed_fitted','marginal_residuals'] as $key) { $compact[$key.'_sample'] = array_map(fn($j) => $result[$key][$j], $indices); unset($compact[$key]); }
    }
    foreach (['fitted', 'residuals', 'n_input', 'n_observations', 'n_dropped', 'retained_rows', 'dropped_rows'] as $key) unset($compact[$key]);
    unset($result);
}
echo json_encode(['version' => PHP_VERSION, 'load_seconds' => $loadSeconds,
    'fit_seconds' => $times, 'sample_indices' => $indices, 'result' => $compact], JSON_THROW_ON_ERROR);
