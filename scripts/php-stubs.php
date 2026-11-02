<?php
// Run with the built extension loaded. Reflection makes signatures/defaults authoritative.
declare(strict_types=1);
if (!extension_loaded('phit')) throw new RuntimeException('Load the phit extension before generating stubs');
$functions = get_extension_funcs('phit'); sort($functions);
$common = ['model'=>"'linear'|'generalized_linear'|'mixed_gaussian'|'mixed_logistic'",'coefficients'=>'list<float>','covariance'=>'list<list<float>>','std_errors'=>'list<float>',
 'p_values'=>'list<float>','conf_int'=>'list<array{float,float}>','fitted'=>'list<float>','residuals'=>'list<float>',
 'log_likelihood'=>'float','df_residual'=>'int','diagnostics'=>'array{converged:bool,solver:string,warnings:list<string>,iterations?:int,evaluations?:int,gradient_max?:float,boundary?:bool,singular?:bool}',
 'n_input'=>'int','n_observations'=>'int','n_dropped'=>'int','retained_rows'=>'list<int>','dropped_rows'=>'list<int>'];
$glm = ['z_statistics'=>'list<float>','deviance'=>'float','null_deviance'=>'float','aic'=>'float','iterations'=>'int','converged'=>'bool'];
$mixed = ['z_statistics'=>'list<float>','fixed_fitted'=>'list<float>','marginal_residuals'=>'list<float>','aic'=>'float',
 'residual_variance'=>'float','sigma'=>'float','method'=>'string','converged'=>'bool'];
$group = ['group_labels'=>'list<int|string>','group_sizes'=>'list<int>','random_effects'=>'list<float>','random_effects_cond_var'=>'list<float>'];
$matrixPurpose=['Phit\fit_ols_matrix'=>'Least-squares fit of a numeric design, like R\'s lm.fit() with the full summary.',
 'Phit\fit_logistic_matrix'=>'Binomial logit fit of a numeric design by Newton steps, like R\'s glm(family = binomial).',
 'Phit\fit_probit_matrix'=>'Binomial probit fit of a numeric design by Fisher scoring, like R\'s glm(family = binomial(link = "probit")).',
 'Phit\fit_poisson_matrix'=>'Poisson log-link fit of a numeric design by Newton steps, like R\'s glm(family = poisson).',
 'Phit\fit_mixed_gaussian_intercept_matrix'=>'Gaussian model with one random intercept, fitted by ML on a numeric design.',
 'Phit\fit_mixed_gaussian_matrix'=>'Gaussian mixed model on a numeric design and explicit random terms, like lme4::lmer().',
 'Phit\fit_mixed_logistic_matrix'=>'Binomial logit model with one random intercept on a numeric design, like lme4::glmer(family = binomial).'];
// The object each model function returns, by its kind of model.
$fitClass=['Phit\fit_ols'=>'LinearFit','Phit\fit_ols_matrix'=>'LinearFit','Phit\fit_generalized_linear'=>'GeneralizedLinearFit',
 'Phit\fit_logistic_matrix'=>'GeneralizedLinearFit','Phit\fit_probit_matrix'=>'GeneralizedLinearFit','Phit\fit_poisson_matrix'=>'GeneralizedLinearFit',
 'Phit\fit_mixed_gaussian'=>'MixedGaussianFit','Phit\fit_mixed_gaussian_matrix'=>'MixedGaussianFit','Phit\fit_mixed_gaussian_intercept_matrix'=>'MixedGaussianFit',
 'Phit\fit_mixed_logistic'=>'MixedLogisticFit','Phit\fit_mixed_logistic_matrix'=>'MixedLogisticFit'];
$models=['Phit\fit_ols_matrix','Phit\fit_logistic_matrix','Phit\fit_probit_matrix','Phit\fit_poisson_matrix','Phit\fit_mixed_gaussian_intercept_matrix','Phit\fit_mixed_gaussian_matrix','Phit\fit_mixed_logistic_matrix'];
// Deprecated aliases, each documented as its replacement plus @deprecated. The 0.3 names were removed in 0.5.0.
$aliases=[];
$families=['normal'=>'Normal(mean, sd)','t'=>"Student's t with df degrees of freedom",'chisquare'=>'Chi-square with df degrees of freedom',
 'f'=>'F with numerator df1 and denominator df2 degrees of freedom','beta'=>'Beta(alpha, beta) on [0, 1]',
 'gamma'=>'Gamma with shape and scale (mean shape*scale)','exponential'=>'Exponential with rate (mean 1/rate)',
 'lognormal'=>'Log-normal: ln X ~ Normal(meanlog, sdlog)','logistic'=>'Logistic(location, scale)',
 'binomial'=>'Binomial: successes in size trials with success probability prob','poisson'=>'Poisson with mean lambda',
 'nbinom'=>'Negative binomial: failures before the size-th success, success probability prob',
 'nbinom_mu'=>'Negative binomial with mean mu and size (variance mu + mu^2/size)'];
$samplers=[
 'Phit\dist_uniform_rng'=>['summary'=>"Uniform(min, max) samples as R's runif(): min + (max - min) * u; min == max gives min.",
  'params'=>['int $n Sample count in [0, 1000000]','int $seed Nonnegative PHP integer; every call restarts its own stream'],'return'=>'list<float>'],
 'Phit\dist_categorical_rng'=>['summary'=>"0-based indices into prob drawn with those probabilities, as R's sample(x, n, replace = TRUE, prob = prob):\n * the first k with u * sum(prob) < prob[0] + ... + prob[k]. Weights are divided by their sum.",
  'params'=>['int $n Sample count in [0, 1000000]','int $seed Nonnegative PHP integer; every call restarts its own stream','list<int|float> $prob Finite, nonnegative weights with a positive sum'],'return'=>'list<int>'],
 'Phit\dist_categorical_rows_rng'=>['summary'=>"One 0-based index per row of prob, drawn with that row's probabilities (e.g. ordered-logit outcomes).\n * Draw i uses the i-th uniform, so equal rows give dist_categorical_rng()'s result.",
  'params'=>['int $seed Nonnegative PHP integer; every call restarts its own stream','list<list<int|float>> $prob At most 1000000 rows of finite, nonnegative weights with a positive sum'],'return'=>'list<int>'],
 'Phit\permutation_rng'=>['summary'=>"size distinct 0-based indices from 0..n-1 in random order, as R's sample(n, size) less one; size null gives all n.\n * Partial Fisher-Yates: step i swaps position i with i + floor(u * (n - i)).",
  'params'=>['int $n Population size in [0, 1000000]','int $seed Nonnegative PHP integer; every call restarts its own stream','int|null $size Draws in [0, n]; null gives a permutation'],'return'=>'list<int>'],
];
$operations=['pdf'=>'density f(x); 0 outside the support','logpdf'=>'log density ln f(x); -INF outside the support',
 'pmf'=>'probability P(X = k); 0 for non-integer or out-of-support k','logpmf'=>'log probability ln P(X = k)',
 'cdf'=>'P(X <= x)','logcdf'=>'ln P(X <= x), accurate after P underflows','sf'=>'survival P(X > x), computed directly',
 'logsf'=>'ln P(X > x), accurate after P underflows','quantile'=>'smallest x with P(X <= x) >= p'];
$descriptive=['Phit\stat_mean'=>'Arithmetic mean.','Phit\stat_var'=>"Variance; type 'sample' divides by n - 1, 'population' by n.",
 'Phit\stat_sd'=>'Standard deviation: square root of stat_var.','Phit\stat_cov'=>"Covariance; type 'sample' (n - 1) or 'population' (n).",
 'Phit\stat_cor'=>'Pearson correlation; NAN when either input is constant.','Phit\stat_quantile'=>"Sample quantile, R's default type 7 (linear interpolation).",
 'Phit\stat_quantile_array'=>'Several type 7 quantiles from one sort.','Phit\stat_median'=>'Median (type 7 quantile at 0.5).',
 'Phit\stat_weighted_mean'=>'Weighted mean sum(w*x)/sum(w); weights must be nonnegative with a positive sum. Negative weights reject before missing deletion.',
 'Phit\stat_weighted_var'=>"Weighted variance; type 'frequency' (W - 1), 'reliability' (W - sum(w^2)/W) or 'population' (W). Negative weights reject before missing deletion."];
$smoothing=[
 'Phit\stat_density'=>['summary'=>"Kernel density estimate as R's density(): linear binning, FFT convolution, then interpolation onto n points from `from` to `to`.\n * bw: 'nrd0' (default), 'nrd' or a number, multiplied by adjust. Kernels: 'gaussian', 'epanechnikov', 'rectangular', 'triangular', 'biweight', 'cosine', 'optcosine'.\n * Weights are not normalized; as in R, weights summing to one are renormalized after missing values are dropped.",
  'return'=>'array{x: list<float>, y: list<float>, bw: float, n: int}'],
 'Phit\stat_bw_nrd0'=>['summary'=>"R's bw.nrd0: 0.9 * min(sd, IQR/1.34) * n^(-1/5), with R's fallbacks when that is zero.",'return'=>null],
 'Phit\stat_bw_nrd'=>['summary'=>"R's bw.nrd: 1.06 * min(sd, IQR/1.34) * n^(-1/5).",'return'=>null],
 'Phit\stat_approx'=>['summary'=>"Interpolate (x, y) at xout as R's approx(): method 'linear' or 'constant' (f weights the right-hand value),\n * rule 1 gives NAN outside [min(x), max(x)], rule 2 the nearest end value; tied x collapse by ties 'mean', 'min', or 'max'.",
  'return'=>'list<float>'],
 'Phit\stat_ecdf'=>['summary'=>"Empirical CDF of x at each q: the proportion of x at or below q, as R's ecdf(x)(q).",'return'=>'list<float>'],
];
$optimizers=[
 'Phit\optimize'=>"Minimize f on [lower, upper] (or maximize, with maximum: true) as R's optimize(): Brent's golden-section and parabolic search.\n * Returns 'minimum' or 'maximum', and the objective f(x).",
 'Phit\uniroot'=>"A root of f in [lower, upper] as R's uniroot(): Brent's method; f(lower) and f(upper) must not have the same sign.\n * Returns the root, f(root), the iterations used and the estimated precision; E_WARNING and iter = maxiter when not converged.",
];
$draws=[
 'Phit\draws_moments'=>['summary'=>"Weighted draw mean and covariance; weights normalized internally. Normalization 'population' or 'unbiased' (reliability weights, never frequency counts). Singular covariance is valid.",
  'params'=>['draws'=>'array<array<int|float>>','weights'=>'array<int|float>|null'],
  'result'=>['mean'=>'list<float>','covariance'=>'list<list<float>>']],
 'Phit\draws_covariance_map'=>['summary'=>'Cholesky/triangular-solve map A: A * source * A^T = target. Finite symmetric positive-definite covariances of equal size; tolerances in docs/draw-numerics.qmd. Failure is explicit; no regularization.',
  'params'=>['sourceCovariance'=>'array<array<int|float>>','targetCovariance'=>'array<array<int|float>>'],
  'result'=>['matrix'=>'list<list<float>>','log_abs_det'=>'float']],
 'Phit\draws_affine'=>['summary'=>'Apply y = A*x + b to each draw: Y = X*A^T + b. Square invertible A; return log(abs(det(A))). Numerical rank tolerance in docs/draw-numerics.qmd.',
  'params'=>['draws'=>'array<array<int|float>>','matrix'=>'array<array<int|float>>','shift'=>'array<int|float>'],
  'result'=>['draws'=>'list<list<float>>','log_abs_det'=>'float']],
];
$naming=['formula'=>'string','response'=>'string','columns'=>'list<string>','terms'=>'list<string>','assign'=>'list<int>','intercept'=>'bool','named'=>'array<string,float>',
 'predvars'=>'array{levels:array<string,list<string>>,poly:array<string,array{alpha:list<float>,norm2:list<float>}>}'];
$olsKeys=['t_statistics'=>'list<float>','rss'=>'float','residual_variance'=>'float','sigma'=>'float','r_squared'=>'float','adj_r_squared'=>'float'];
$randomShape='list<array{label:string,group:string,columns:list<string>,correlated:bool,z:list<list<float>>,groups:list<int|string>}>';
$formulas=[
 'Phit\model_matrix'=>['summary'=>"R-compatible model.matrix() for a formula over named columns: treatment contrasts (first level is the reference),\n * contr.poly for ordered factors, R column names/term order, inline transforms, offset() and random-effect bars.\n * Unused factor levels are kept, as in model.matrix(); x includes the \"(Intercept)\" column when present.",
  'result'=>['y'=>'list<float>|list<array{float,float}>|null','response_levels?'=>'list<string>','x'=>'list<list<float>>','columns'=>'list<string>','intercept'=>'bool','terms'=>'list<string>','assign'=>'list<int>',
   'formula'=>'string','response'=>'string|null','offset'=>'list<float>|null','random'=>$randomShape,'n_input'=>'int','n_observations'=>'int','n_dropped'=>'int','retained_rows'=>'list<int>','dropped_rows'=>'list<int>']],
 'Phit\fit_ols'=>['summary'=>"Linear model from a formula, like R's lm(): builds the model matrix and fits by least squares, weighted least squares\n * when weights are given (as lm(weights = )), with offset() terms.\n * Factor levels unused after deletion are dropped, as lm() does. Rank deficiency raises 1004 instead of R's NA coefficients.",
  'result'=>$common+$olsKeys+$naming],
 'Phit\fit_generalized_linear'=>['summary'=>"Generalized linear model from a formula, like R's glm(): gaussian/identity (the OLS core), binomial/logit, binomial/probit\n * and poisson/log (the matrix GLM cores). Binomial responses: 0/1, logical, factor/string (first level = failure),\n * proportions with trial weights, or cbind(successes, failures). The empty link selects the canonical link.",
  'result'=>$common+$glm+['family'=>'string','link'=>'string']+$naming+['t_statistics?'=>'list<float>','rss?'=>'float','residual_variance?'=>'float','sigma?'=>'float','r_squared?'=>'float','adj_r_squared?'=>'float']],
 'Phit\fit_mixed_gaussian'=>['summary'=>"Gaussian mixed model from an lme4-style formula, like lme4::lmer(): fixed terms as in fit_ols and random bars\n * (x | g), (x || g), (1 | g1) + (1 | g2), (1 | g1:g2), (1 | g1/g2) mapped onto the fit_mixed_gaussian_matrix core. REML by default.\n * Groups and effects follow first appearance after deletion, not lme4's sorted levels; terms keep formula order.
 * Each random term's effects_cond_var holds one k x k conditional covariance per group, as ranef(condVar = TRUE).",
  'result'=>$common+$mixed+['n_parameters'=>'int','bic'=>'float','theta'=>'list<float>','singular'=>'bool','optimizer'=>'string','optimizer_implementation'=>'string','evaluations'=>'int',
   'random'=>'list<array{covariance:list<list<float>>,effects:list<list<float>>,effects_cond_var:list<list<list<float>>>,group_labels:list<int|string>,group_sizes:list<int>,label:string,group:string,columns:list<string>,correlated:bool}>']+$naming],
 'Phit\fit_mixed_logistic'=>['summary'=>"Binomial logit random-intercept model from a formula, like lme4::glmer(family = binomial): exactly one (1 | g) term over\n * the fit_mixed_logistic_matrix core (Laplace by default, adaptive quadrature with quadraturePoints > 1). It fits only the\n * binomial family with the logit link, so it takes no family or link. Responses as in fit_generalized_linear.
 * random_effects_cond_var holds each conditional mode's variance, as ranef(condVar = TRUE).",
  'result'=>$common+['z_statistics'=>'list<float>','fixed_fitted'=>'list<float>','linear_predictor'=>'list<float>','aic'=>'float','bic'=>'float','n_parameters'=>'int']+$group+
   ['n_groups'=>'int','random_intercept_sd'=>'float','random_intercept_variance'=>'float','neg2_log_likelihood'=>'float','quadrature_points'=>'int','integration'=>'string','method'=>'string','family'=>'string','link'=>'string','optimizer'=>'string','converged'=>'bool','evaluations'=>'int','gradient_max'=>'float','boundary'=>'bool','singular'=>'bool',
   'random'=>'list<array{label:string,group:string,columns:list<string>}>']+$naming],
];
$predict=['summary'=>"Predictions for new data from a fit_ols (formula), fit_generalized_linear, fit_mixed_gaussian or fit_mixed_logistic result, like R's predict():\n * the fit's formula is rebuilt over newdata with the fitted factor levels and poly() basis (its predvars key).\n * A level the fit never saw is an error. Rows with missing predictors predict null and are listed in missing_rows.\n * type 'response' applies the inverse link; seFit uses the delta method on that scale. Intervals ('confidence' or\n * 'prediction', t with df_residual) are for fit_ols fits. Mixed fits add random effects of known groups (random: false\n * gives population predictions; allowNewLevels predicts unseen groups at zero). GLM standard errors use the\n * covariance at the final coefficients (docs/r-compatibility.qmd).",
  'result'=>['fit'=>'list<float|null>','se_fit?'=>'list<float|null>','df?'=>'int','residual_scale?'=>'float','lower?'=>'list<float|null>','upper?'=>'list<float|null>','level'=>'float|null','n_input'=>'int','n_predicted'=>'int','missing_rows'=>'list<int>']];
$io=['Phit\read_csv'=>['summary'=>"Read a delimited text file the way phibble's PHP reader (after readr::read_delim()) does: same records,\n * names, guessed types, values and problem messages. Contract and phibble/readr differences: docs/io.qmd.",
  'options'=>"array{delimiter?: string, quote?: string, escape_double?: bool, escape_backslash?: bool, na?: list<string>, trim_ws?: bool,\n *   skip?: int, n_max?: ?int, comment?: string, skip_empty_rows?: bool, col_names?: bool, types?: array<int, '?'|'l'|'i'|'d'|'c'|'D'|'T'|'_'>,\n *   guess_max?: int, decimal_mark?: string, grouping_mark?: ?string, block_size?: int}",
  'result'=>['names'=>'list<string>','columns'=>"list<null|array{type: 'bool'|'int'|'float'|'string'|'date'|'datetime', values: list<bool|int|float|string|null>, failed: int}>",'problems'=>'list<string>']]];
$text="<?php\n// Generated by scripts/php-stubs.php from the loaded native API.\n// IDE/static-analysis declarations only: do not require this file at runtime.\n// Statistical definitions and supported structures: docs/contracts.qmd.\n// Every function is in the Phit namespace: use function Phit\\fit_ols;\n\nnamespace Phit;\n\n";
$api=[];
foreach ($functions as $name) {
 $r = new ReflectionFunction($name); $params=[];$schema=[];
 foreach ($r->getParameters() as $p) {
  $part=(string)$p->getType().' $'.$p->getName();
  $meta=['name'=>$p->getName(),'type'=>(string)$p->getType(),'optional'=>$p->isOptional()];
  if ($p->isDefaultValueAvailable()) { $part.=' = '.($p->getDefaultValue()===[]?'[]':var_export($p->getDefaultValue(),true));$meta['default']=$p->getDefaultValue(); }
  $params[]=$part;$schema[]=$meta;
 }
 $api[$name]=['parameters'=>$schema,'return_type'=>(string)$r->getReturnType()];
 $alias=$name;$name=$aliases[$alias]??$name;
 if($alias!==$name)$api[$alias]['deprecated']=$name;
 if (isset($draws[$name])) {
  $contract=$draws[$name];
  $api[$alias]['result_keys']=array_keys($contract['result']);sort($api[$alias]['result_keys']);
  $text.="/**\n * {$contract['summary']}\n * Rows are draws; columns are parameters. Preserves iteration order; no row deletion or intercept insertion.\n * null/NAN/INF and nonnumeric elements are rejected.\n";
  foreach($contract['params'] as $param=>$type)$text.=" * @param $type \$$param\n";
  $text.=" * @return array{";
  $fields=[];foreach($contract['result'] as $key=>$type)$fields[]="$key: $type";
  $text.=implode(', ',$fields)."}\n * @throws \\Exception Input/numerical error (codes 1001–1005).\n */\n";
 } elseif (isset($io[$name])) {
  $contract=$io[$name];
  $api[$alias]['result_keys']=array_keys($contract['result']);sort($api[$alias]['result_keys']);
  $text.="/**\n * {$contract['summary']}\n * Dates are int days since 1970-01-01; datetimes are int microseconds since the epoch (UTC); NA is null.\n * failed counts values that did not parse under the column's type (they are null). Skipped ('_') columns are null.\n";
  $text.=" * @param string \$path A readable file (bytes are passed through; no encoding validation)\n";
  $text.=" * @param {$contract['options']} \$options readr's defaults when omitted\n";
  $text.=" * @return array{";
  $fields=[];foreach($contract['result'] as $key=>$type)$fields[]="$key: $type";
  $text.=implode(', ',$fields)."}\n * @throws \\ValueError Missing or unreadable file, or an invalid option.\n */\n";
 } elseif (isset($formulas[$name])) {
  $contract=$formulas[$name];
  $api[$alias]['result_keys']=array_map(fn($k)=>rtrim($k,'?'),array_keys($contract['result']));sort($api[$alias]['result_keys']);
  $text.="/**\n * {$contract['summary']}\n * Formula syntax and R differences: docs/formulas.qmd. Rows with null/NAN in any variable the formula uses are\n * dropped with one E_WARNING (missing 'raise' rejects them); retained_rows/dropped_rows are original positions.\n";
  $text.=" * @param string \$formula R formula, such as 'y ~ log(x) + f * z + offset(log(t))'\n";
  $text.=" * @param array<string,list<int|float|bool|string|null>|array{levels:list<string>,codes:list<int|null>,ordered:bool}>|object \$data Named columns or factor descriptors; objects use toModelData(), else toColumns()\n";
  if($name==='Phit\fit_generalized_linear'||$alias==='Phit\fit_glmer')$text.=" * @param string \$family 'gaussian', 'binomial', or 'poisson'\n * @param string \$link '' (canonical), 'identity', 'logit', 'probit', or 'log'\n";
  if($name!=='Phit\model_matrix')$text.=" * @param array<int|float|null>|string|null \$weights Positive prior weights aligned with the original rows, or a column name\n";
  $text.=isset($fitClass[$name])?" * @return {$fitClass[$name]} A fit object, read like this array:\n * array{\n":" * @return array{\n";
  foreach($contract['result'] as $key=>$type)$text.=" *   $key: $type,\n";
  $text.=" * }\n * @throws \\Exception Statistical or input error (codes 1001–1008); unsupported functions/families/random terms raise 1007.\n */\n";
 } elseif ($name==='Phit\predict') {
  $api[$alias]['result_keys']=array_map(fn($k)=>rtrim($k,'?'),array_keys($predict['result']));sort($api[$alias]['result_keys']);
  $text.="/**\n * {$predict['summary']}\n";
  $text.=" * @param Fit|array<string,mixed> \$fit A formula fit, or its toArray() array, with the keys `formula`, `coefficients`, `covariance`, `columns`, and `predvars`\n";
  $text.=" * @param array<string,list<int|float|bool|string|null>|array{levels:list<string>,codes:list<int|null>,ordered:bool}>|object \$newdata Named columns, as for the fit; the response may be absent\n";
  $text.=" * @param string \$type 'link' (linear predictor) or 'response'\n * @param string \$interval 'none', 'confidence', or 'prediction'\n * @param bool \$random Mixed fits: include the random effects of known groups, as lme4's `re.form = NULL`; false predicts at the population level, as `re.form = NA`\n";
  $text.=" * @return array{\n";
  foreach($predict['result'] as $key=>$type)$text.=" *   $key: $type,\n";
  $text.=" * }\n * @throws \\Exception Input error (1001), malformed fit, new levels, or unsupported options (1007).\n */\n";
 } elseif ($name==='Phit\summary') {
  $text.="/**\n * Summary.\n * @param Fit|array<string,mixed> \$fit\n * @return string\n * @throws \\Exception\n */\n";
 } elseif ($name==='Phit\tidy') {
  $text.="/**\n * Tidy.\n * @param Fit|array<string,mixed> \$fit\n * @return Table Columns term, estimate, std_error, statistic, and p_value, or with effects 'ran_vals' group, level, term, estimate, and std_error; confInt adds conf_low and conf_high\n * @throws \\Exception\n */\n";
 } elseif (isset($optimizers[$name])) {
  $text.="/**\n * {$optimizers[$name]}\n * f receives one float and returns an int or float; an exception from f propagates. Nonfinite values during the search\n * are replaced by +/-PHP_FLOAT_MAX, as R does, with one E_WARNING.\n";
  $text.=" * @param callable(float): (int|float) \$f\n * @param float|null \$tol Default PHP_FLOAT_EPSILON ** 0.25, as R\n";
  $text.=$name==='Phit\optimize'
   ? " * @return array{minimum: float, objective: float}|array{maximum: float, objective: float}\n"
   : " * @return array{root: float, f_root: float, iter: int, estim_prec: float}\n";
  $text.=" * @throws \\Exception Input error (code 1001).\n */\n";
 } elseif (isset($smoothing[$name])) {
  $text.="/**\n * {$smoothing[$name]['summary']}\n * Data arrays: null/NAN are missing, rejected by missing 'raise' (default) or removed by 'drop'; infinite values raise 1003.\n";
  foreach ($r->getParameters() as $p) {
   $type=(string)$p->getType();
   if (in_array($p->getName(),['xout','q'],true)) $text.=' * @param array<int|float|null> $'.$p->getName()." Evaluation points; null or NAN gives NAN\n";
   elseif ($type==='array'||$type==='?array') $text.=' * @param array<int|float|null>'.($type==='?array'?'|null':'').' $'.$p->getName()."\n";
   elseif ($p->getName()==='bw') $text.=" * @param string|float \$bw 'nrd0', 'nrd', or a positive bandwidth\n";
  }
  if ($smoothing[$name]['return']) $text.=" * @return {$smoothing[$name]['return']}\n";
  $text.=" * @throws \\Exception Input error (codes 1001–1003).\n */\n";
 } elseif (isset($descriptive[$name])) {
  $text.="/**\n * {$descriptive[$name]}\n * null/NAN are missing: missing 'raise' (default) rejects them, 'drop' removes them (pairwise for two arrays).\n";
  $text.=" * Nonfinite arithmetic raises 1003; constant-input correlation remains NAN.\n";
  foreach ($r->getParameters() as $p) if ((string)$p->getType()==='array') $text.=' * @param array<int|float|null> $'.$p->getName()."\n";
  if ($name==='Phit\stat_quantile_array') $text.=" * @return list<float>\n";
  $text.=" * @throws \\Exception Input error (codes 1001–1003).\n */\n";
 } elseif (isset($samplers[$name])) {
  $text.="/**\n * {$samplers[$name]['summary']}\n * Local ChaCha20 stream restarted at the seed, one uniform per draw; same seed/arguments reproduce prefixes within the pinned version.\n";
  foreach($samplers[$name]['params'] as $param=>$doc)$text.=" * @param $doc\n";
  $text.=" * @return {$samplers[$name]['return']}\n * @throws \\Exception Invalid input (1001), nonfinite input (1003) or more than 1000000 draws (1007).\n */\n";
 } elseif (preg_match('/^Phit\\\\dist_(.+)_rng$/',$name,$m) && isset($families[$m[1]])) {
  $text.="/**\n * {$families[$m[1]]}: explicitly seeded batch samples.\n * Local ChaCha20 + inverse CDF; same seed/parameters reproduce prefixes within the pinned version.\n * @param int \$n Sample count in [0, 1000000]; empty batches still validate parameters.\n * @param int \$seed Nonnegative PHP integer; every call restarts its own stream.\n * @return list<float> Finite samples; discrete counts are floats.\n * @throws \\Exception Invalid input (1001), nonfinite parameter/sample (1003) or unsupported count/size (1007).\n */\n";
 } elseif (preg_match('/^Phit\\\\dist_(.+)_(pdf|logpdf|pmf|logpmf|cdf|logcdf|sf|logsf|quantile)(_array)?$/',$name,$m) && isset($families[$m[1]])) {
  $text.="/**\n * {$families[$m[1]]}: {$operations[$m[2]]}.\n";
  if (isset($m[3])) {
   $first=$r->getParameters()[0]->getName();
   $text.=" * Batch form: one native call; returns one value per element in iteration order (keys are not kept).\n * @param array<int|float> \$$first\n * @return list<float>\n";
  }
  $text.=" * @throws \\Exception Invalid parameter (1001), NAN input (1003) or numerical failure (1006).\n */\n";
 } elseif (in_array($name,$models,true)) {
  $keys=$common;
  if ($name==='Phit\fit_ols_matrix') $keys+=['t_statistics'=>'list<float>','rss'=>'float','residual_variance'=>'float','sigma'=>'float','r_squared'=>'float','adj_r_squared'=>'float'];
  elseif (in_array($name,['Phit\fit_logistic_matrix','Phit\fit_probit_matrix','Phit\fit_poisson_matrix'],true)) $keys+=$glm;
  elseif ($name==='Phit\fit_mixed_gaussian_intercept_matrix') $keys+=$mixed+$group+['evaluations'=>'int','n_groups'=>'int','random_intercept_variance'=>'float','icc'=>'float','boundary'=>'bool'];
  elseif ($name==='Phit\fit_mixed_gaussian_matrix') $keys+=$mixed+['n_parameters'=>'int','bic'=>'float','theta'=>'list<float>','singular'=>'bool','optimizer'=>'string','optimizer_implementation'=>'string','evaluations'=>'int',
   'random'=>'list<array{covariance:list<list<float>>,effects:list<list<float>>,effects_cond_var:list<list<list<float>>>,group_labels:list<int|string>,group_sizes:list<int>}>'];
  else $keys+=['z_statistics'=>'list<float>','fixed_fitted'=>'list<float>','linear_predictor'=>'list<float>','aic'=>'float','bic'=>'float','n_parameters'=>'int']+$group+
   ['n_groups'=>'int','random_intercept_sd'=>'float','random_intercept_variance'=>'float','neg2_log_likelihood'=>'float','quadrature_points'=>'int','integration'=>'string','method'=>'string','family'=>'string','link'=>'string','optimizer'=>'string','converged'=>'bool','evaluations'=>'int','gradient_max'=>'float','boundary'=>'bool','singular'=>'bool'];
  $api[$alias]['result_keys']=array_keys($keys);sort($api[$alias]['result_keys']);
  $text.="/**\n * {$matrixPurpose[$name]}\n * Predictor matrix: rows are observations. A fixed intercept is prepended by default.\n * null/NAN rows follow the missing policy; weights/offset are aligned with original rows.\n * @param array<array<int|float|null>> \$x\n * @param array<int|float|null> \$y\n * @param bool \$intercept False uses x exactly (including any caller-supplied ones).\n";
  if(in_array($name,['Phit\fit_mixed_gaussian_intercept_matrix','Phit\fit_mixed_logistic_matrix'],true))$text.=" * @param array<int|string|null> \$groups\n";
  if($name==='Phit\fit_ols_matrix')$text.=" * @param array<int|float|null>|null \$weights Positive weights: weighted least squares, as lm(weights = )\n";
  if($name==='Phit\fit_mixed_gaussian_matrix')$text.=" * @param array<array{z:array<array<int|float|null>>,groups:array<int|string|null>,correlated?:bool}> \$random\n";
  $text.=" * @return {$fitClass[$name]} A fit object, read like this array:\n * array{\n";
  foreach($keys as $key=>$type)$text.=" *   $key: $type,\n";
  $text.=" * }\n * @throws \\Exception Statistical or input error (codes 1001–1008).\n */\n";
 }
 if($alias!==$name)$text=preg_replace('~ \*/\n$~'," * @deprecated Since phit 0.4.0; use \\$name(). Removed in phit 0.5.\n */\n",$text);
 $name=$alias;
 $returns=$fitClass[$name]??($name==='Phit\tidy'?'Table':(string)$r->getReturnType());
 $params=array_map(fn($p)=>preg_replace('~^mixed \$fit$~','Fit|array $fit',$p),$params);
 $text.='function '.substr($name,strlen('Phit\\')).'('.implode(', ',$params).'): '.$returns." {}\n\n";
}
// Classes, by the same reflection.
$classes=['Phit\CsvReader'=>['summary'=>"Reads a CSV file a chunk of rows at a time, as phibble's DataFrame::csvChunks() (after readr::read_delim_chunked()):\n * only the current chunk is held. Options are read_csv()'s; types not given are guessed from the first chunk (guess_max\n * of its rows) and every later chunk is parsed as those types. Problem rows are numbered from the start of the file.",
  'methods'=>['__construct'=>"Opens the file and reads its header line.\n   * @param string \$path A readable file\n   * @param {$io['Phit\read_csv']['options']} \$options\n   * @throws \\ValueError Missing or unreadable file, or an invalid option.",
   'names'=>"Column names: the header's, or X1..Xn with col_names false; [] for an empty file.\n   * @return list<string>",
   'next'=>"The next chunk of at most \$rows rows in read_csv()'s shape, with the 1-based position of its first row and its\n   * row count; null once no rows remain. A chunk without rows comes only to carry problems found after the last row.\n   * @return null|array{position: int, rows: int, names: list<string>, columns: {$io['Phit\read_csv']['result']['columns']}, problems: list<string>}\n   * @throws \\ValueError rows below 1, or a read error."]]];
// The fit and table objects share these method descriptions.
$fitMethods=['__construct'=>"Not available: fits come from the model functions.",
 '__toString'=>"The fit printed in the layout of R's print(summary(fit)), as summary() returns it.",
 'summary'=>"The fit printed in the layout of R's print(summary(fit)).",
 'tidy'=>"The coefficient table as columns, as tidy() returns it.\n   * @param bool \$confInt Add conf_low and conf_high columns\n   * @param float \$level Confidence level of the intervals, in (0, 1)\n   * @param string \$effects 'fixed' for the coefficient table, or 'ran_vals' for a mixed fit's random effects\n   * @throws \\Exception Code 1001 for an invalid level or effects.",
 'predict'=>"Predictions for new data, as predict() returns them for this fit.\n   * @param array<string,list<int|float|bool|string|null>>|object \$newdata Named columns; the response may be absent\n   * @param string \$type 'link' or 'response'\n   * @param bool \$seFit Add standard errors\n   * @param string \$interval 'none', 'confidence', or 'prediction'\n   * @param float \$level Interval level\n   * @param bool \$random Mixed fits: include the random effects of known groups\n   * @param bool \$allowNewLevels Mixed fits: predict unseen groups at the population level\n   * @return array<string,mixed>\n   * @throws \\Exception As predict().",
 'toArray'=>"The fit as the array the model function computed.\n   * @return array<string,mixed>",
 'jsonSerialize'=>"The same JSON as the array.\n   * @return array<string,mixed>",
 'toMimeBundle'=>"A Jupyter display bundle: the printed fit as text/plain and the array as application/json.\n   * @return array{'text/plain': string, 'application/json': array<string,mixed>}",
 '__debugInfo'=>"The array, for var_dump() and print_r().\n   * @return array<string,mixed>",
 'offsetExists'=>"Whether the key is present and not null, as isset() on the array.",
 'offsetGet'=>"The key's value, as on the array; a missing key warns and gives null.",
 'offsetSet'=>"Not available: fits are read-only.\n   * @throws \\LogicException Always.",
 'offsetUnset'=>"Not available: fits are read-only.\n   * @throws \\LogicException Always.",
 'count'=>"The number of keys.",
 'current'=>"The value at the iterator's position.",
 'key'=>"The key at the iterator's position.",
 'next'=>"Moves the iterator to the next key.",
 'rewind'=>"Moves the iterator to the first key.",
 'valid'=>"Whether the iterator's position holds a key."];
foreach(['LinearFit'=>'a linear model (fit_ols(), fit_ols_matrix())','GeneralizedLinearFit'=>'a generalized linear model (fit_generalized_linear() and the logistic, probit, and Poisson matrix functions)',
 'MixedGaussianFit'=>'a Gaussian mixed model (fit_mixed_gaussian() and its matrix functions)','MixedLogisticFit'=>'a binomial logit mixed model (fit_mixed_logistic(), fit_mixed_logistic_matrix())'] as $c=>$what)
 $classes["Phit\\$c"]=['summary'=>"The fit of $what. echo prints its summary; reading a key works as on the result array;\n * the fit is read-only, and toArray() returns the array.",'methods'=>$fitMethods];
$classes['Phit\Table']=['summary'=>"Named columns, as tidy() returns them. echo prints them in the layout of R's print.data.frame(); reading a\n * column works as on the array; the table is read-only, and toColumns() or toArray() returns the columns.",
 'methods'=>['__construct'=>"Not available: tables come from tidy().",'__toString'=>"The columns in the layout of R's print.data.frame().",
  'toArray'=>"The columns, name => list.\n   * @return array<string,list<string|float>>",'toColumns'=>"The columns, name => list, for phibble's DataFrame::fromColumns() and phit's data arguments.\n   * @return array<string,list<string|float>>",
  'count'=>"The number of rows.",'offsetGet'=>"The column's values; a missing column warns and gives null."]+$fitMethods];
$text.="/**\n * A model fit: the result of every model function. echo prints its summary, and reading a key works as on the result\n * array. The four classes implementing it are LinearFit, GeneralizedLinearFit, MixedGaussianFit, and MixedLogisticFit.\n */\ninterface Fit extends \\ArrayAccess, \\Iterator, \\Countable, \\JsonSerializable, \\Stringable\n{\n";
foreach(['summary(): string','tidy(bool $confInt = false, float $level = 0.95, string $effects = \'fixed\'): Table','predict(mixed $newdata, string $type = \'link\', bool $seFit = false, string $interval = \'none\', float $level = 0.95, bool $random = true, bool $allowNewLevels = false): array','toArray(): array','toMimeBundle(): array'] as $m)
 $text.="  public function $m;\n";
$text.="}\n\n";
$api['Phit\Fit']=['interface'=>true,'extends'=>(new ReflectionClass('Phit\Fit'))->getInterfaceNames()];
foreach(get_declared_classes() as $class){
 $rc=new ReflectionClass($class);
 if($rc->getExtensionName()!=='phit')continue;
 if(!isset($classes[$class]))throw new RuntimeException("Undocumented class $class");
 $api[$class]=['methods'=>[]];
 $implements=array_values(array_diff($rc->getInterfaceNames(),['Traversable']));
 if(in_array('Phit\Fit',$implements,true))$implements=['Phit\Fit'];
 $implements=array_map(fn($i)=>str_starts_with($i,'Phit\\')?substr($i,5):'\\'.$i,$implements);
 $text.="/**\n * {$classes[$class]['summary']}\n */\nfinal class ".substr($class,strlen('Phit\\')).($implements?' implements '.implode(', ',$implements):'')."\n{\n";
 $methods=$rc->getMethods();usort($methods,fn($a,$b)=>[$a->getName()!=='__construct',$a->getName()]<=>[$b->getName()!=='__construct',$b->getName()]);
 foreach($methods as $m){
  $params=[];$schema=[];
  foreach($m->getParameters() as $p){
   $part=(string)$p->getType().' $'.$p->getName();
   $meta=['name'=>$p->getName(),'type'=>(string)$p->getType(),'optional'=>$p->isOptional()];
   if($p->isDefaultValueAvailable()){$part.=' = '.($p->getDefaultValue()===[]?'[]':var_export($p->getDefaultValue(),true));$meta['default']=$p->getDefaultValue();}
   $params[]=$part;$schema[]=$meta;
  }
  $api[$class]['methods'][$m->getName()]=['parameters'=>$schema,'return_type'=>(string)$m->getReturnType()];
  $doc=$classes[$class]['methods'][$m->getName()]??throw new RuntimeException("Undocumented method $class::{$m->getName()}");
  $return=$m->getName()==='tidy'?': Table':($m->getReturnType()===null?'':': '.$m->getReturnType());
  $hidden=$m->getName()==='__construct'&&$class!=='Phit\CsvReader';
  $text.="  /**\n   * $doc\n   */\n  ".($hidden?'private':'public')." function {$m->getName()}(".implode(', ',$params).")$return {}\n\n";
 }
 $text=rtrim($text)."\n}\n\n";
}
// Replace each function's description and error line with the text in scripts/php-docs.php. A deprecated alias
// takes its replacement's text.
$docs=require __DIR__.'/php-docs.php';
$text=preg_replace_callback('~/\*\*((?:(?!\*/).)*?)\*/\nfunction (\w+)\(~s',function(array $m)use($docs):string{
 [$all,$body,$function]=$m;
 $target=preg_match('~@deprecated .*?\\\\?Phit\\\\(\w+)\(\)~',$body,$d)?$d[1]:$function;
 if(isset($docs['descriptions'][$target])){
  $lines=$docs['descriptions'][$target];
 }elseif(preg_match('~^dist_(nbinom_mu|[a-z]+?)_((?:log)?(?:pdf|pmf|cdf|sf)|quantile|rng)(_array)?$~',$target,$f)&&isset($docs['families'][$f[1]])){
  $family=$docs['families'][$f[1]];
  if($f[2]==='rng')$lines=['Seeded samples from '.$family.'.','- Returns a list of `n` floats; discrete counts are floats.',$docs['stream']];
  else{[$sentence,$note]=$docs['operations'][$f[2]];$lines=array_values(array_filter([sprintf($sentence,$family),$note,$f[3]??''?$docs['batch']:null]));}
 }else throw new RuntimeException("No description for $function");
 $group=match(true){
  str_starts_with($target,'dist_')&&in_array($target,['dist_uniform_rng','dist_categorical_rng','dist_categorical_rows_rng'],true),$target==='permutation_rng'=>'sampler',
  str_ends_with($target,'_rng')=>'dist_rng',str_starts_with($target,'dist_')=>'dist',
  in_array($target,['fit_ols','fit_generalized_linear','fit_mixed_gaussian','fit_mixed_logistic','model_matrix'],true)=>'formula',
  str_starts_with($target,'fit_')=>'matrix',str_starts_with($target,'stat_')=>'stat',str_starts_with($target,'draws_')=>'draws',
  in_array($target,['optimize','uniroot'],true)=>'optimize',default=>$target};
 [$class,$error]=$docs['throws'][$group]??throw new RuntimeException("No error text for $function");
 $tags=substr($body,strpos($body,"\n * @"));
 $tags=preg_replace('~\n \* @throws [^\n]*~',"\n * @throws $class $error",$tags);
 return "/**\n * ".implode("\n * ",$lines).$tags."*/\nfunction $function(";
},$text);
// Describe every parameter: a docblock line that ends at the parameter name gets the description from
// scripts/php-param-docs.php, and a parameter without a line gets one, typed from the signature.
$paramDocs=require __DIR__.'/php-param-docs.php';
$text=preg_replace_callback('~(/\*\*(?:(?!\*/).)*?\*/)\nfunction (\w+)\(([^)]*)\)~s',function(array $m)use($paramDocs):string{
 [$all,$doc,$function,$signature]=$m;
 $family=preg_match('~^dist_(nbinom_mu|[a-z]+)_~',$function,$f)?$f[1]:null;
 foreach(array_filter(array_map('trim',explode(', ',$signature))) as $param){
  if(!preg_match('~^(\S+) \$(\w+)~',$param,$pm))continue;
  [, $type,$name]=$pm;
  $line='~(\* @param [^\n]*\$'.$name.')([ \t]*)\n~';
  if(preg_match($line,$doc,$found)&&$found[2]===''){
   $text=$paramDocs["$function:$name"]??($family?($paramDocs["$family:$name"]??null):null)??$paramDocs[php_param_group($function).":$name"]??null;
   if($text===null)throw new RuntimeException("No description for $function \$$name");
   $doc=preg_replace($line,'$1 '.str_replace(['\\','$'],['\\\\','\\$'],$text)."\n",$doc,1);
  }elseif(!preg_match('~\$'.$name.'\b~',$doc)){
   $text=$paramDocs["$function:$name"]??($family?($paramDocs["$family:$name"]??null):null)??$paramDocs[php_param_group($function).":$name"]??null;
   if($text===null)throw new RuntimeException("No description for $function \$$name");
   $insert=" * @param $type \$$name $text\n";
   $doc=preg_match('~\n \* @(return|throws|deprecated)~',$doc)?preg_replace('~(\n \* @(?:return|throws|deprecated))~',"\n".rtrim($insert,"\n").'$1',$doc,1):preg_replace('~\n \*/$~',"\n".rtrim($insert,"\n")."\n */",$doc);
  }
 }
 return $doc."\nfunction $function($signature)";
},$text);
$text=rtrim($text)."\n";
$root=dirname(__DIR__);$stub="$root/stubs/phit.php";$apiPath="$root/tests/fixtures/php-api.json";
if(in_array('--check',$argv,true)){
 if(file_get_contents($stub)!==$text)throw new RuntimeException('PHP stubs drifted; regenerate scripts/php-stubs.php');
 if(json_decode(file_get_contents($apiPath),true,flags:JSON_THROW_ON_ERROR)!==$api)throw new RuntimeException('Native signatures/defaults changed; review compatibility before updating php-api.json');
 echo "PHP signatures, defaults and stubs verified\n";
}else{
 file_put_contents($stub,$text);
 if(in_array('--write-api',$argv,true))file_put_contents($apiPath,json_encode($api,JSON_PRETTY_PRINT|JSON_THROW_ON_ERROR|JSON_PRESERVE_ZERO_FRACTION)."\n");
 echo "PHP stubs generated\n";
}
