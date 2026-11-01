<?php
declare(strict_types=1);
// Saved synthetic apartments: area (m²), bedrooms, monthly rent.
// CSV preparation belongs to application code; the native API takes numbers.
$file = fopen(__DIR__ . '/../docs/data/housing.csv', 'r');
fgetcsv($file, escape: ''); // Header.
$x = $y = [];
while (($row = fgetcsv($file, escape: '')) !== false) {
    $x[] = [(float) $row[0]]; // Observed area; the fixed intercept is added by default.
    $y[] = (float) $row[2];
}
fclose($file);
$result = Phit\fit_ols_matrix(x: $x, y: $y);
printf("%-12s %12s %12s %12s\n", 'Term', 'Estimate', 'Std. error', 'p-value');
foreach ($result['coefficients'] as $j => $coefficient) {
    printf("%-12s %12.6f %12.6f %12.6g\n", $j === 0 ? 'Intercept' : 'Area (m²)', $coefficient, $result['std_errors'][$j], $result['p_values'][$j]);
}
printf("R² = %.6f; adjusted R² = %.6f; sigma = %.6f; residual df = %d\n", $result['r_squared'], $result['adj_r_squared'], $result['sigma'], $result['df_residual']);
printf("Gaussian log-likelihood = %.6f\n", $result['log_likelihood']);
