<?php
// Vignette for phit.phpstats.org: a linear model with prediction intervals.
declare(strict_types=1);

use function Phit\{fit_lm, predict, read_csv};

return [
    'inputs' => [
        'bedrooms' => ['type' => 'select', 'label' => 'Bedrooms', 'options' => [1 => '1', 2 => '2', 3 => '3', 4 => '4'], 'default' => 2],
    ],
    'run' => function (array $input, string $data): array {
        $bedrooms = (int) $input['bedrooms'];
        $csv = read_csv($data . '/rents.csv');
        $housing = array_combine($csv['names'], array_column($csv['columns'], 'values'));

        $fit = fit_lm('monthly_rent ~ area_m2 + bedrooms', $housing);
        $areas = range(30, 125, 5);
        $p = predict($fit, ['area_m2' => $areas, 'bedrooms' => array_fill(0, count($areas), $bedrooms)], interval: 'prediction');

        $curve = [];
        foreach ($areas as $i => $area) {
            $curve[] = ['area_m2' => $area, 'fit' => $p['fit'][$i], 'lower' => $p['lower'][$i], 'upper' => $p['upper'][$i]];
        }
        $points = [];
        foreach ($housing['area_m2'] as $i => $area) {
            $points[] = ['area_m2' => $area, 'monthly_rent' => $housing['monthly_rent'][$i], 'bedrooms' => $housing['bedrooms'][$i]];
        }

        return [
            ['type' => 'text', 'markdown' => "`fit_lm()` fits monthly rent on floor area and bedrooms, as R's `lm()` would. "
                . "`predict()` evaluates the fit on new data, here with 95% prediction intervals for a {$bedrooms}-bedroom flat."],
            ['type' => 'code', 'chain' => "use function Phit\\{fit_lm, predict};\n\n"
                . "\$fit = fit_lm('monthly_rent ~ area_m2 + bedrooms', \$housing);\n"
                . "\$p = predict(\$fit, ['area_m2' => \$areas, 'bedrooms' => \$beds], interval: 'prediction');", 'pipe' => null],
            ['type' => 'metrics', 'values' => ['R²' => $fit['r_squared'], 'Residual σ' => $fit['sigma'], 'Observations' => $fit['n_observations']]],
            ['type' => 'model', 'title' => 'Coefficients', 'result' => $fit],
            ['type' => 'chart', 'title' => "Predicted rent for {$bedrooms} bedrooms", 'spec' => [
                '$schema' => 'https://vega.github.io/schema/vega-lite/v5.json',
                'width' => 'container', 'height' => 320,
                'layer' => [
                    ['data' => ['values' => $curve], 'mark' => ['type' => 'area', 'opacity' => 0.2],
                        'encoding' => ['x' => ['field' => 'area_m2', 'type' => 'quantitative', 'title' => 'Floor area (m²)'],
                            'y' => ['field' => 'lower', 'type' => 'quantitative', 'title' => 'Monthly rent'], 'y2' => ['field' => 'upper']]],
                    ['data' => ['values' => $points], 'mark' => ['type' => 'point', 'filled' => true, 'opacity' => 0.6],
                        'encoding' => ['x' => ['field' => 'area_m2', 'type' => 'quantitative'], 'y' => ['field' => 'monthly_rent', 'type' => 'quantitative'],
                            'color' => ['field' => 'bedrooms', 'type' => 'ordinal', 'title' => 'Bedrooms']]],
                    ['data' => ['values' => $curve], 'mark' => ['type' => 'line', 'strokeWidth' => 2],
                        'encoding' => ['x' => ['field' => 'area_m2', 'type' => 'quantitative'], 'y' => ['field' => 'fit', 'type' => 'quantitative']]],
                ],
            ]],
            ['type' => 'table', 'title' => 'Predictions', 'columns' => ['area_m2', 'fit', 'lower', 'upper'],
                'rows' => array_values(array_filter($curve, fn ($row) => $row['area_m2'] % 15 === 0))],
        ];
    },
];
