<?php
declare(strict_types=1);
// Synthetic students: observed study hours, practice score, and pass/fail.
$file = fopen(__DIR__ . '/../docs/data/learning.csv', 'r');
fgetcsv($file, escape: '');
$x = $y = [];
while (($row = fgetcsv($file, escape: '')) !== false) {
    $x[] = [(float) $row[0], (float) $row[1]];
    $y[] = (float) $row[2];
}
fclose($file);
$result = Phit\fit_logistic_matrix(x: $x, y: $y);
$labels = ['Intercept', 'Study hours', 'Practice score'];
printf("%-16s %12s %12s %12s %12s\n", 'Term', 'Estimate', 'Std. error', 'z', 'p-value');
foreach ($result['coefficients'] as $j => $coefficient) {
    printf("%-16s %12.6f %12.6f %12.6f %12.6g\n", $labels[$j], $coefficient, $result['std_errors'][$j], $result['z_statistics'][$j], $result['p_values'][$j]);
}
printf("n = %d; iterations = %d; log-likelihood = %.6f; deviance = %.6f; AIC = %.6f\n", $result['n_observations'], $result['iterations'], $result['log_likelihood'], $result['deviance'], $result['aic']);
printf("First student's fitted probability of passing: %.6f\n", $result['fitted'][0]);
