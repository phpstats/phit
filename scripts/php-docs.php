<?php
// Docblock descriptions and error lines for stubs/phit.php, used by scripts/php-stubs.php.
// A description is a one-sentence purpose, then behavior lines that start with "- ".
declare(strict_types=1);

$missingFormula = "- Rows with `null` or `NAN` in a variable the formula uses are dropped with one warning; `missing: 'raise'` rejects them instead. `retained_rows` and `dropped_rows` give the original row positions.";
$missingMatrix = "- Rows with `null` or `NAN` in `x`, `y`, or another aligned input are dropped with one warning; `missing: 'raise'` rejects them instead. `retained_rows` and `dropped_rows` give the original row positions.";
$intercept = '- An intercept column is prepended unless `intercept` is false, in which case `x` is used exactly.';
$missingStat = "- `null` and `NAN` are missing: `missing: 'raise'` rejects them, and `missing: 'drop'` removes them, pairwise for two arrays.";
$infinite = '- Infinite input, or arithmetic that overflows, throws.';
$draws = '- Rows are draws and columns are parameters. Row order is kept; no row is dropped and no intercept is added.';
$drawsInput = '- `null`, `NAN`, `INF`, and nonnumeric elements are rejected.';
$stream = '- Each call starts its own ChaCha20 stream from `seed`, one uniform per draw. The same seed and arguments reproduce the same draws, and a longer request extends a shorter one.';

$families = [
    'normal' => 'the normal distribution with `mean` and `sd`',
    't' => "Student's t distribution with `df` degrees of freedom",
    'chisquare' => 'the chi-square distribution with `df` degrees of freedom',
    'f' => 'the F distribution with `df1` and `df2` degrees of freedom',
    'beta' => 'the beta distribution with shapes `alpha` and `beta`',
    'gamma' => 'the gamma distribution with `shape` and `scale`',
    'exponential' => 'the exponential distribution with `rate`',
    'lognormal' => 'the log-normal distribution with `meanlog` and `sdlog`',
    'logistic' => 'the logistic distribution with `location` and `scale`',
    'binomial' => 'the binomial distribution with `size` trials and success probability `prob`',
    'poisson' => 'the Poisson distribution with mean `lambda`',
    'nbinom' => 'the negative binomial distribution with `size` and success probability `prob`',
    'nbinom_mu' => 'the negative binomial distribution with `size` and mean `mu`',
];
$operations = [
    'pdf' => ['Density f(x) of %s.', '- 0 outside the support.'],
    'logpdf' => ['Log density ln f(x) of %s.', '- `-INF` outside the support.'],
    'pmf' => ['Probability P(X = k) of %s.', '- 0 for a non-integer or out-of-support `k`.'],
    'logpmf' => ['Log probability ln P(X = k) of %s.', '- `-INF` for a non-integer or out-of-support `k`.'],
    'cdf' => ['Lower-tail probability P(X ≤ x) of %s.', null],
    'logcdf' => ['Log lower-tail probability ln P(X ≤ x) of %s.', '- Accurate after P(X ≤ x) underflows.'],
    'sf' => ['Upper-tail probability P(X > x) of %s.', '- Computed directly, not as 1 − P(X ≤ x).'],
    'logsf' => ['Log upper-tail probability ln P(X > x) of %s.', '- Accurate after P(X > x) underflows.'],
    'quantile' => ['Quantile of %s: the smallest x with P(X ≤ x) ≥ p.', null],
];
$batch = '- Batch form: one call over a list, returning one value per element in input order, without the keys.';

$descriptions = [
    'fit_ols' => [
        "Fits a linear model from a formula, like R's `lm()`.",
        '- Builds the model matrix from named columns and fits by least squares, or by weighted least squares when `weights` is given, as `lm(weights = )` does.',
        '- `offset()` terms in the formula are added to the linear predictor.',
        '- Factor levels left unused after missing rows are dropped are removed, as `lm()` does.',
        "- A rank-deficient design throws instead of returning R's `NA` coefficients.",
        $missingFormula,
    ],
    'fit_generalized_linear' => [
        "Fits a generalized linear model from a formula, like R's `glm()`.",
        "- Families and links: `gaussian` with `identity`, `binomial` with `logit` or `probit`, and `poisson` with `log`. An empty `link` selects the canonical link.",
        '- A binomial response can be 0/1 numbers, booleans, a factor or string column (the first level is failure), proportions with integer trial weights, or `cbind(successes, failures)`.',
        $missingFormula,
    ],
    'model_matrix' => [
        "Builds the numeric design of a formula over named columns, like R's `model.matrix()`.",
        '- Factors use treatment contrasts with the first level as the reference; ordered factors use `contr.poly`.',
        '- Column names, term order, inline transforms, `offset()`, and random-effect bars follow R.',
        '- Unused factor levels are kept, as in `model.matrix()`, and `x` includes the `(Intercept)` column when present.',
        $missingFormula,
    ],
    'predict' => [
        "Predicts new data from a `fit_ols()`, `fit_generalized_linear()`, `fit_mixed_gaussian()`, or `fit_mixed_logistic()` result, like R's `predict()`.",
        "- The fit's formula is rebuilt over `newdata` with the fitted factor levels and `poly()` basis, from the fit's `predvars` key. A level the fit never saw throws.",
        '- A row with a missing predictor predicts `null` and is listed in `missing_rows`.',
        "- `type: 'response'` applies the inverse link, and `seFit` then uses the delta method on that scale.",
        "- `interval: 'confidence'` or `'prediction'` adds `lower` and `upper` for `fit_ols()` fits, using t with `df_residual`.",
        '- Mixed fits add the random effects of known groups. `random: false` gives population predictions, and `allowNewLevels` predicts unseen groups at the population level.',
        "- GLM standard errors use the covariance at the final coefficients, which differs slightly from R's `predict.glm()`.",
    ],
    'fit_ols_matrix' => [
        "Fits a linear model by least squares on a numeric design, like R's `lm.fit()` with the full summary.",
        '- `weights` gives weighted least squares, as `lm(weights = )` does.',
        $intercept,
        $missingMatrix,
    ],
    'fit_logistic_matrix' => [
        "Fits a binomial logit model on a numeric design by Newton steps, like R's `glm(family = binomial)`.",
        $intercept,
        $missingMatrix,
    ],
    'fit_probit_matrix' => [
        "Fits a binomial probit model on a numeric design by Fisher scoring, like R's `glm(family = binomial(link = \"probit\"))`.",
        $intercept,
        $missingMatrix,
    ],
    'fit_poisson_matrix' => [
        "Fits a Poisson log-link model on a numeric design by Newton steps, like R's `glm(family = poisson)`.",
        $intercept,
        $missingMatrix,
    ],
    'fit_mixed_gaussian' => [
        'Fits a Gaussian mixed model from an lme4 formula, like `lme4::lmer()`.',
        '- Fixed terms are as in `fit_ols()`. Random terms can be `(x | g)`, `(x || g)`, `(1 | g1) + (1 | g2)`, `(1 | g1:g2)`, and `(1 | g1/g2)`.',
        '- Fits by REML unless `reml` is false.',
        "- Groups and effects follow first appearance after missing rows are dropped, not lme4's sorted levels; terms keep formula order.",
        "- Each random term's `effects_cond_var` holds one k × k conditional covariance per group, as `ranef(condVar = TRUE)`.",
        $missingFormula,
    ],
    'fit_mixed_logistic' => [
        'Fits a binomial logit model with one random intercept from a formula, like `lme4::glmer(family = binomial)`.',
        '- The formula has exactly one `(1 | g)` term. The model is binomial with the logit link only, so the function takes no family or link.',
        '- Uses the Laplace approximation by default, and adaptive quadrature when `quadraturePoints` is above 1.',
        '- Responses are as in `fit_generalized_linear()`.',
        "- `random_effects_cond_var` holds the variance of each conditional mode, as `ranef(condVar = TRUE)`.",
        $missingFormula,
    ],
    'fit_mixed_gaussian_matrix' => [
        'Fits a Gaussian mixed model on a numeric design with explicit random terms, like `lme4::lmer()`.',
        '- Fits by ML unless `reml` is true.',
        $intercept,
        $missingMatrix,
    ],
    'fit_mixed_gaussian_intercept_matrix' => [
        'Fits a Gaussian model with one random intercept by ML on a numeric design.',
        '- Has no random slopes, additional grouping terms, offsets, weights, or REML; use `fit_mixed_gaussian_matrix()` for those.',
        $intercept,
        $missingMatrix,
    ],
    'fit_mixed_logistic_matrix' => [
        'Fits a binomial logit model with one random intercept on a numeric design, like `lme4::glmer(family = binomial)`.',
        '- Uses the Laplace approximation by default, and adaptive quadrature when `quadraturePoints` is above 1.',
        $intercept,
        $missingMatrix,
    ],
    'stat_mean' => ['Arithmetic mean.', $missingStat, $infinite],
    'stat_median' => ['Median: the type 7 quantile at 0.5.', $missingStat, $infinite],
    'stat_var' => ["Variance: `type: 'sample'` divides by n − 1, and `'population'` by n.", $missingStat, $infinite],
    'stat_sd' => ['Standard deviation: the square root of `stat_var()`.', $missingStat, $infinite],
    'stat_quantile' => ["Sample quantile by R's default type 7, linear interpolation.", $missingStat, $infinite],
    'stat_quantile_array' => ['Several type 7 quantiles from one sort.', $missingStat, $infinite],
    'stat_weighted_mean' => [
        'Weighted mean: sum(w·x) / sum(w).',
        '- Weights must be nonnegative with a positive sum. A negative weight is rejected before missing values are removed.',
        $missingStat, $infinite,
    ],
    'stat_weighted_var' => [
        "Weighted variance: `type: 'frequency'` divides by W − 1, `'reliability'` by W − sum(w²)/W, and `'population'` by W, with W = sum(w).",
        '- A negative weight is rejected before missing values are removed.',
        $missingStat, $infinite,
    ],
    'stat_cov' => ["Covariance of two aligned arrays: `type: 'sample'` divides by n − 1, and `'population'` by n.", $missingStat, $infinite],
    'stat_cor' => ['Pearson correlation of two aligned arrays.', '- Returns `NAN` when either input is constant.', $missingStat, $infinite],
    'stat_density' => [
        "Kernel density estimate, like R's `density()`.",
        '- Bins the data linearly, convolves with the kernel by FFT, and interpolates onto `n` points from `from` to `to`.',
        '- `bw` is `nrd0`, `nrd`, or a positive number, multiplied by `adjust`.',
        '- Weights are used as given; as in R, weights that sum to one are renormalized after missing values are removed.',
        $missingStat, $infinite,
    ],
    'stat_bw_nrd0' => ["Silverman's rule-of-thumb bandwidth, like R's `bw.nrd0()`: 0.9 · min(sd, IQR/1.34) · n^(−1/5), with R's fallbacks when that is zero.", $missingStat, $infinite],
    'stat_bw_nrd' => ["Scott's variation of the rule-of-thumb bandwidth, like R's `bw.nrd()`: 1.06 · min(sd, IQR/1.34) · n^(−1/5).", $missingStat, $infinite],
    'stat_approx' => [
        "Interpolates the points (x, y) at `xout`, like R's `approx()`.",
        "- `method: 'linear'` interpolates linearly; `'constant'` uses a step function in which `f` weights the right-hand value.",
        '- Outside [min(x), max(x)], `rule: 1` returns `NAN` and `rule: 2` the value at the nearer end.',
        "- Tied x values collapse by `ties`: `'mean'`, `'min'`, or `'max'`.",
        '- A `null` or `NAN` in `xout` returns `NAN`.',
        $missingStat, $infinite,
    ],
    'stat_ecdf' => [
        "Empirical CDF of `x` at each `q`: the proportion of `x` at or below `q`, as R's `ecdf(x)(q)`.",
        '- A `null` or `NAN` in `q` returns `NAN`.',
        $missingStat, $infinite,
    ],
    'optimize' => [
        "Minimizes `f` on [`lower`, `upper`], or maximizes it with `maximum: true`, like R's `optimize()`.",
        "- Uses Brent's golden-section and parabolic search, step for step as R does.",
        "- Returns `minimum` (or `maximum`) and `objective`, the value of `f` there.",
        '- `f` takes one float and returns an int or float. An exception thrown by `f` propagates.',
        '- A nonfinite value during the search is replaced by the largest finite value, as in R, with one warning.',
    ],
    'uniroot' => [
        "Finds a root of `f` in [`lower`, `upper`], like R's `uniroot()`.",
        "- Uses Brent's method, step for step as R does. `f(lower)` and `f(upper)` must not have the same sign.",
        '- Returns `root`, `f_root`, `iter`, and `estim_prec`. When it runs out of iterations it warns, sets `iter` to `maxiter`, and returns its last estimate.',
        '- `f` takes one float and returns an int or float. An exception thrown by `f` propagates.',
        '- A nonfinite value during the search is replaced by the largest finite value, as in R, with one warning.',
    ],
    'draws_moments' => [
        'Weighted mean and covariance of posterior draws.',
        "- Weights are normalized internally. `normalization: 'unbiased'` treats them as reliability weights, never frequency counts.",
        '- A singular covariance is a valid result.',
        $draws, $drawsInput,
    ],
    'draws_covariance_map' => [
        'Matrix A with A · source · Aᵀ = target, from Cholesky factors and triangular solves.',
        '- Both covariances must be finite, symmetric, positive definite, and of equal size. Failure throws; there is no regularization.',
        $drawsInput,
    ],
    'draws_affine' => [
        'Applies y = A·x + b to every draw: Y = X·Aᵀ + b.',
        '- `matrix` must be square and invertible. Also returns log |det(A)|.',
        $draws, $drawsInput,
    ],
    'dist_uniform_rng' => [
        "Seeded uniform samples on [`min`, `max`], like R's `runif()`: min + (max − min) · u.",
        '- `min` equal to `max` returns `min`.',
        $stream,
    ],
    'dist_categorical_rng' => [
        "Seeded 0-based category indices drawn with probabilities `prob`, like R's `sample(x, n, replace = TRUE, prob = prob)`.",
        '- Weights are divided by their sum. Each draw is the first k with u · sum(prob) < prob[0] + … + prob[k].',
        $stream,
    ],
    'dist_categorical_rows_rng' => [
        'One seeded 0-based category index per row of `prob`, drawn with that row\'s probabilities, as for ordered-logit outcomes.',
        '- Draw i uses the i-th uniform, so equal rows give the result of `dist_categorical_rng()`.',
        $stream,
    ],
    'permutation_rng' => [
        "Seeded sample of `size` distinct 0-based indices from 0 to n − 1 in random order, like R's `sample(n, size)` minus one.",
        '- `size: null` gives a permutation of all n.',
        '- Uses a partial Fisher–Yates shuffle: step i swaps position i with i + floor(u · (n − i)).',
        $stream,
    ],
    'summary' => [
        "Prints a model fit in the layout of R's `print(summary(fit))`, as text ending in a newline.",
        '- Takes the result of any model function: OLS and GLM fits, formula or matrix, and Gaussian and logistic mixed fits.',
        "- The coefficient table follows R's `printCoefmat(digits = 4)` without significance stars. A linear model adds the residual standard error, R-squared, and the F statistic; a GLM adds the dispersion, deviances, and AIC; a mixed model adds its fit criteria and the random-effect variances, as lme4 prints them.",
        '- Matrix fits name their coefficients x1, x2, and so on, and leave out the lines that need a formula.',
    ],
    'tidy' => [
        "Returns a fit's coefficient table as columns, like broom's `tidy()`.",
        '- The columns are `term`, `estimate`, `std_error`, `statistic`, and `p_value`. `conf_int: true` adds `conf_low` and `conf_high` at `level`.',
        '- Intervals are Wald intervals: t with `df_residual` degrees of freedom when the fit reports t statistics, and the normal distribution otherwise.',
        '- Mixed fits give the fixed effects only; `summary()` prints the variance components.',
    ],
    'read_csv' => [
        "Reads a delimited text file into typed columns, with the records, names, guessed types, values, and problem messages of phibble's PHP reader and `readr::read_delim()`.",
        '- Dates are int days since 1970-01-01, and datetimes are int microseconds since the epoch, in UTC. `NA` values are `null`.',
        "- `failed` counts values that did not parse under the column's type; they are `null`. Skipped columns are `null`.",
    ],
];

$throws = [
    'dist' => ['\\Exception', 'Code 1001 for an invalid parameter, 1003 for a `NAN` input, or 1006 for a numerical failure.'],
    'dist_rng' => ['\\Exception', 'Code 1001 for invalid input, 1003 for a nonfinite parameter or sample, or 1007 for more than 1,000,000 draws or an unsupported size.'],
    'sampler' => ['\\Exception', 'Code 1001 for invalid input, 1003 for nonfinite input, or 1007 for more than 1,000,000 draws.'],
    'formula' => ['\\Exception', 'A code from 1001 to 1008 for an input or statistical error; 1007 for an unsupported function, family, or random term.'],
    'matrix' => ['\\Exception', 'A code from 1001 to 1008 for an input or statistical error.'],
    'predict' => ['\\Exception', 'Code 1001 for invalid input, a malformed fit, or a new factor level, or 1007 for an unsupported option.'],
    'stat' => ['\\Exception', 'Code 1001, 1002, or 1003 for an input error.'],
    'draws' => ['\\Exception', 'A code from 1001 to 1005 for an input or numerical error.'],
    'optimize' => ['\\Exception', 'Code 1001 for an input error.'],
    'summary' => ['\\Exception', 'Code 1001 when `fit` is not the result of a model function.'],
    'tidy' => ['\\Exception', 'Code 1001 when `fit` is not the result of a model function or `level` is outside (0, 1).'],
    'read_csv' => ['\\ValueError', 'A missing or unreadable file, or an invalid option.'],
];

return ['descriptions' => $descriptions, 'families' => $families, 'operations' => $operations, 'batch' => $batch,
        'stream' => $stream, 'throws' => $throws];
