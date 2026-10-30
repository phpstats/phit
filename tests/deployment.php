<?php
// Same assertions through CLI and an actual PHP-FPM FastCGI request.
declare(strict_types=1);
try {
    if (!extension_loaded('phit')) throw new RuntimeException('phit is not enabled');
    preg_match('/^version\s*=\s*"([^"]+)"/m', file_get_contents(__DIR__.'/../crates/php-extension/Cargo.toml'), $version);
    if (phpversion('phit') !== ($version[1] ?? null)) throw new RuntimeException('unexpected extension version');
    if (Phit\dist_normal_cdf(0.0) !== 0.5) throw new RuntimeException('native call failed');
    $ols = Phit\fit_ols([[1], [2], [3], [4], [5]], [2.1, 4.2, 5.8, 8.1, 10.2]);
    if (abs($ols['coefficients'][0] - 0.05) > 1e-10 || abs($ols['coefficients'][1] - 2.01) > 1e-10) {
        throw new RuntimeException('automatic-intercept OLS failed');
    }
    $exposure = [1, 2, 4, 8]; $counts = [1, 4, 3, 12];
    $poisson = Phit\fit_poisson(array_fill(0, 4, []), $counts, offset: array_map(fn($e) => log($e), $exposure));
    if (abs($poisson['coefficients'][0] - log(20 / 15)) > 1e-10) throw new RuntimeException('Poisson exposure fit failed');
    if (PHP_SAPI !== 'cli') header('Content-Type: application/json');
    echo json_encode(['ok' => true, 'sapi' => PHP_SAPI, 'php' => PHP_VERSION,
        'extension' => phpversion('phit'), 'zts' => (bool) PHP_ZTS,
        'coefficients' => $ols['coefficients'], 'poisson_rate' => exp($poisson['coefficients'][0])], JSON_THROW_ON_ERROR), "\n";
} catch (Throwable $error) {
    if (PHP_SAPI !== 'cli') { http_response_code(500); header('Content-Type: application/json'); }
    echo json_encode(['ok' => false, 'error' => $error->getMessage()], JSON_THROW_ON_ERROR), "\n";
    exit(1);
}
