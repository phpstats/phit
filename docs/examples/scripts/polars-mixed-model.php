<?php
require 'vendor/autoload.php';

use Phiz\Plot;

use function Phit\fit_lmer;
use function Phpolars\scan_csv;

// 1. Read three columns with Polars and copy them into PHP arrays once.
$schools = scan_csv('schools.csv')
    ->select(['school', 'study_hours', 'score'])
    ->collect();
$data = $schools->toColumns();

// 2. Fit a random intercept per school by REML. Keep the fitted values.
$fit = fit_lmer('score ~ study_hours + (1 | school)', $data);
$data['fitted'] = $fit['fitted'];

foreach ($fit['columns'] as $i => $name) {
    printf("%-12s %7.3f (se %.3f)\n", $name,
        $fit['coefficients'][$i], $fit['std_errors'][$i]);
}
$school = $fit['random'][0];
printf("%s: school SD %.3f, residual SD %.3f\n", $fit['method'],
    sqrt($school['covariance'][0][0]), $fit['sigma']);
printf("%d students in %d schools\n",
    $fit['n_observations'], count($school['group_labels']));

// 3. Chart the students as points and each school's fitted line.
$chart = Plot::of($data)
    ->aes(x: 'study_hours', y: 'score')
    ->point(alpha: 0.3)
    ->line(aes: ['y' => 'fitted', 'group' => 'school'], alpha: 0.6)
    ->labs(x: 'Study hours', y: 'Test score');

file_put_contents('schools.html', $chart->html('Scores by school'));
