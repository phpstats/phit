<?php
require 'vendor/autoload.php';

use Phiz\Plot;

use function Phit\{fit_mixed_gaussian, summary};
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

echo summary($fit);

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
