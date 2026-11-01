<?php
// Parameter descriptions for stubs/phit.php, used by scripts/php-stubs.php where a docblock gives none.
// A key "function:param" applies to one function; "group:param" to a group of functions (see php_param_group()).
declare(strict_types=1);

function php_param_group(string $function): string
{
    return match (true) {
        str_starts_with($function, 'dist_') => 'dist',
        str_starts_with($function, 'stat_') => 'stat',
        str_starts_with($function, 'draws_') => 'draws',
        str_starts_with($function, 'fit_'), $function === 'model_matrix', $function === 'predict' => 'model',
        default => 'other',
    };
}

return [
    // Distributions.
    'dist:x' => 'Point at which to evaluate; ±INF allowed',
    'dist:values' => 'Points at which to evaluate; ±INF allowed, and null or NAN rejects the batch',
    'dist:k' => 'Count at which to evaluate',
    'dist:counts' => 'Counts at which to evaluate; null or NAN rejects the batch',
    'dist:p' => 'Probability in [0, 1]',
    'dist:probabilities' => 'Probabilities in [0, 1]; null or NAN rejects the batch',
    'dist:mean' => 'Mean',
    'dist:sd' => 'Standard deviation, > 0',
    'dist:df' => 'Degrees of freedom, > 0',
    'dist:df1' => 'Numerator degrees of freedom, > 0',
    'dist:df2' => 'Denominator degrees of freedom, > 0',
    'dist:alpha' => 'First shape parameter, > 0',
    'dist:beta' => 'Second shape parameter, > 0',
    'dist:shape' => 'Shape, > 0',
    'dist:scale' => 'Scale, > 0',
    'dist:rate' => 'Rate, > 0; the mean is 1 / rate',
    'dist:meanlog' => 'Mean of log X',
    'dist:sdlog' => 'Standard deviation of log X, > 0',
    'dist:location' => 'Location: the mean and median',
    'dist:lambda' => 'Mean, ≥ 0',
    'dist:mu' => 'Mean, ≥ 0',
    'dist:size' => 'Dispersion: a positive real, not a trial count',
    'dist:prob' => 'Success probability in (0, 1]',
    'dist:min' => 'Lower bound',
    'dist:max' => 'Upper bound, ≥ min',
    // Binomial size and prob differ from the negative binomial's.
    'binomial:size' => 'Number of trials, an integer in [0, 2^53]',
    'binomial:prob' => 'Success probability in [0, 1]',
    'gamma:scale' => 'Scale, > 0; the mean is shape × scale',

    // Models.
    'model:x' => 'Predictor matrix: one row per observation and one numeric column per predictor; null or NAN is missing',
    'model:y' => 'Numeric response, one value per row; null or NAN is missing',
    'model:missing' => "'drop' removes rows with null or NAN in any variable the model uses and warns once; 'raise' rejects them",
    'model:weights' => 'Positive prior weights aligned with the rows',
    'model:offset' => 'Known part of the linear predictor, aligned with the rows; null is zero',
    'model:groups' => 'Group label per row, an integer or a string; integer 1 and string "1" are different groups',
    'model:reml' => 'true for REML, false for ML',
    'model:start' => 'Starting relative covariance factors, in the units of the random columns; null starts diagonal factors at one and off-diagonals at zero',
    'model:maxEvaluations' => 'Largest number of optimizer evaluations',
    'model:tolerance' => 'BOBYQA final trust-region radius, in (0, 0.1]',
    'model:optimizer' => "'bobyqa' or 'nelder_mead'",
    'model:objectiveTolerance' => 'Nelder–Mead stopping tolerance: the standard deviation of the simplex objective values',
    'model:quadraturePoints' => '1 for the Laplace approximation, or 2 to 61 for adaptive Gauss–Hermite quadrature',
    'model:random' => "Random terms, each an array with 'z' (n × k design), 'groups' (one label per row), and optional 'correlated' (default true; false is (x || g))",
    'model:seFit' => 'Also return standard errors of the predictions; fixed-effect fits only',
    'model:level' => 'Confidence level of the interval',
    'model:allowNewLevels' => 'Predict a group the fit never saw at the population level instead of raising 1001',
    'fit_logistic_matrix:y' => '0/1 outcomes, or proportions with integer trial weights; null or NAN is missing',
    'fit_probit_matrix:y' => '0/1 outcomes, or proportions with integer trial weights; null or NAN is missing',
    'fit_mixed_logistic_matrix:y' => '0/1 outcomes, or proportions with integer trial weights; null or NAN is missing',
    'fit_poisson_matrix:y' => 'Nonnegative integer counts, one per row; null or NAN is missing',
    'fit_logistic_matrix:weights' => 'Positive integer trial counts aligned with the rows; null is one trial per row',
    'fit_probit_matrix:weights' => 'Positive integer trial counts aligned with the rows; null is one trial per row',
    'fit_mixed_logistic_matrix:weights' => 'Positive integer trial counts, at most 1,000,000 per row; null is one trial per row',

    // Statistics.
    'stat:values' => 'Data; null and NAN are missing',
    'stat:x' => 'Data; null and NAN are missing',
    'stat:y' => 'Second variable, aligned with the first; null and NAN are missing',
    'stat:missing' => "'raise' rejects null and NAN; 'drop' removes them, pairwise for aligned inputs",
    'stat:p' => 'Probability in [0, 1]',
    'stat:probabilities' => 'Probabilities in [0, 1]',
    'stat:type' => "'sample' divides by n − 1, 'population' by n",
    'stat:weights' => 'Nonnegative weights with a positive sum, aligned with the values',
    'stat_weighted_var:type' => "'frequency' divides by W − 1, 'reliability' by W − sum(w²)/W, 'population' by W, with W = sum(w)",
    'stat_density:weights' => 'Nonnegative weights aligned with x, used as given; null weights all points equally',
    'stat_density:adjust' => 'Multiplier applied to the bandwidth',
    'stat_density:kernel' => "'gaussian', 'epanechnikov', 'rectangular', 'triangular', 'biweight', 'cosine', or 'optcosine'",
    'stat_density:n' => 'Number of output points',
    'stat_density:from' => 'Left end of the output grid; null extends the data range by cut bandwidths',
    'stat_density:to' => 'Right end of the output grid; null extends the data range by cut bandwidths',
    'stat_density:cut' => 'Bandwidths beyond the data range for the default grid',
    'stat_approx:x' => 'Abscissae of the points; null and NAN are missing',
    'stat_approx:y' => 'Ordinates, aligned with x; null and NAN are missing',
    'stat_approx:method' => "'linear' or 'constant'",
    'stat_approx:rule' => '1 returns NAN outside the data range; 2 returns the value at the nearer end',
    'stat_approx:ties' => "'mean', 'min', or 'max' for tied x values",
    'stat_approx:f' => "For method 'constant', the weight of the right-hand value, in [0, 1]",

    // Posterior draws.
    'draws:draws' => 'Draws: one row per draw and one column per parameter',
    'draws:weights' => 'Nonnegative weights, one per draw, normalized internally; null weights draws equally',
    'draws:normalization' => "'population' or 'unbiased' (reliability weights)",
    'draws:matrix' => 'Square invertible matrix A',
    'draws:shift' => 'Shift b, one value per parameter',
    'draws:sourceCovariance' => 'Symmetric positive-definite covariance of the source draws',
    'draws:targetCovariance' => 'Symmetric positive-definite target covariance, of the same size',

    // Optimization and root finding.
    'optimize:f' => 'Objective: takes one float and returns an int or float',
    'uniroot:f' => 'Function whose root is sought: takes one float and returns an int or float',
    'other:lower' => 'Lower end of the search interval',
    'other:upper' => 'Upper end of the search interval',
    'other:maximum' => 'Maximize instead of minimize',
    'other:maxiter' => 'Largest number of iterations',
];
