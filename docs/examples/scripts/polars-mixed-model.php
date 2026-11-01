<?php
require 'vendor/autoload.php';

use Phiz\Plot;

use function Phit\fit_mixed_gaussian;
use function Phpolars\scan_csv;

// Read three columns with Polars, then copy them into PHP arrays once.
$data = scan_csv('schools.csv')
    ->select(['school', 'study_hours', 'score'])
    ->collect()
    ->toColumns();

// A random intercept per school, fitted by REML. Keep the conditional fitted values for the chart.
$fit = fit_mixed_gaussian('score ~ study_hours + (1 | school)', $data);
$data['fitted'] = $fit['fitted'];

foreach ($fit['columns'] as $i => $name) {
    printf("%-12s %7.3f (se %.3f)\n", $name, $fit['coefficients'][$i], $fit['std_errors'][$i]);
}
printf("school SD %.3f, residual SD %.3f\n", sqrt($fit['random'][0]['covariance'][0][0]), $fit['sigma']);

// Students as points, and each school's fitted values joined into a line.
$chart = Plot::of($data)
    ->aes(x: 'study_hours', y: 'score')
    ->point(alpha: 0.3)
    ->line(aes: ['y' => 'fitted', 'group' => 'school'], alpha: 0.6)
    ->labs(x: 'Study hours', y: 'Test score');

file_put_contents('schools.html', $chart->html('Scores by school'));
