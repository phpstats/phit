<?php
require 'vendor/autoload.php';

use Phiz\Plot;

use function Phit\fit_mixed_gaussian;
use function Phpolars\scan_csv;

$data = scan_csv('schools.csv')
    ->select(['school', 'study_hours', 'score'])
    ->collect()
    ->toColumns();

// A random intercept per school, fitted by REML.
$fit = fit_mixed_gaussian(
    'score ~ study_hours + (1 | school)',
    $data,
);
$data['fitted'] = $fit['fitted'];

foreach ($fit['columns'] as $i => $name) {
    printf(
        "%-12s %7.3f (se %.3f)\n",
        $name,
        $fit['coefficients'][$i],
        $fit['std_errors'][$i],
    );
}
printf(
    "school SD %.3f, residual SD %.3f\n",
    sqrt($fit['random'][0]['covariance'][0][0]),
    $fit['sigma'],
);

// Students as points, and one fitted line per school.
$chart = Plot::of($data)
    ->aes(x: 'study_hours', y: 'score')
    ->point(alpha: 0.3)
    ->line(
        aes: ['y' => 'fitted', 'group' => 'school'],
        alpha: 0.6,
    )
    ->labs(x: 'Study hours', y: 'Test score')
    ->theme(baseSize: 15)
    ->size('container', 380);

file_put_contents(
    'schools.html',
    $chart->html('Scores by school'),
);
