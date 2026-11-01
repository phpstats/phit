<?php
declare(strict_types=1);
// Illustrative clinic counts, with unequal days of observation.
$visitors = [82, 104, 96, 131, 87, 115, 142, 98];
$days = [1, 2, 1, 3, 2, 1, 2, 3];
$calls = [5, 12, 4, 22, 8, 8, 19, 16];
$result = Phit\fit_poisson_matrix(
    x: array_map(fn($v) => [$v], $visitors),
    y: $calls,
    offset: array_map(fn($d) => log($d), $days),
);
printf("Log-rate intercept: %.6f\nVisitors coefficient: %.6f\n", ...$result['coefficients']);
foreach ($result['fitted'] as $i => $count) {
    printf("Clinic %d: expected %.3f calls over %d days (%.3f/day)\n", $i + 1, $count, $days[$i], $count / $days[$i]);
}
printf("Log-likelihood: %.6f; AIC: %.6f\n", $result['log_likelihood'], $result['aic']);
