<?php
require 'vendor/autoload.php';

use Phiz\Plot;

use function Phit\fit_mixed_gaussian;
use function Phpolars\scan_csv;

$data = scan_csv(
    'https://raw.githubusercontent.com/phpstats/datasets/main/csv/sleepstudy.csv',
)
    ->select(['Reaction', 'Days', 'Subject'])
    ->collect()
    ->toColumns();

// A random intercept per subject, fitted by REML.
$fit = fit_mixed_gaussian(
    'Reaction ~ Days + (1 | Subject)',
    $data,
);
$data['fitted'] = $fit['fitted'];

foreach ($fit['columns'] as $i => $name) {
    printf(
        "%-12s %7.2f (se %.2f)\n",
        $name,
        $fit['coefficients'][$i],
        $fit['std_errors'][$i],
    );
}
printf(
    "subject SD %.2f, residual SD %.2f\n",
    sqrt($fit['random'][0]['covariance'][0][0]),
    $fit['sigma'],
);

// Each subject's days as points, and one fitted line per subject.
$chart = Plot::of($data)
    ->aes(x: 'Days', y: 'Reaction')
    ->point(alpha: 0.4)
    ->line(
        aes: ['y' => 'fitted', 'group' => 'Subject'],
        alpha: 0.6,
    )
    ->labs(x: 'Days of sleep deprivation', y: 'Reaction time (ms)')
    ->theme(baseSize: 15)
    ->size('container', 380);

file_put_contents(
    'sleepstudy.html',
    $chart->html('Reaction time by subject'),
);
