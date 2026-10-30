<?php
// Run with the built extension loaded. Reflection makes signatures/defaults authoritative.
declare(strict_types=1);
if (!extension_loaded('phit')) throw new RuntimeException('Load the phit extension before generating stubs');
$functions = get_extension_funcs('phit'); sort($functions);
$common = ['coefficients'=>'list<float>','covariance'=>'list<list<float>>','std_errors'=>'list<float>',
 'p_values'=>'list<float>','conf_int'=>'list<array{float,float}>','fitted'=>'list<float>','residuals'=>'list<float>',
 'log_likelihood'=>'float','df_residual'=>'int','diagnostics'=>'array{converged:bool,solver:string,warnings:list<string>,iterations?:int,evaluations?:int,gradient_max?:float,boundary?:bool,singular?:bool}',
 'n_input'=>'int','n_observations'=>'int','n_dropped'=>'int','retained_rows'=>'list<int>','dropped_rows'=>'list<int>'];
$glm = ['z_statistics'=>'list<float>','deviance'=>'float','null_deviance'=>'float','aic'=>'float','iterations'=>'int','converged'=>'bool'];
$mixed = ['z_statistics'=>'list<float>','fixed_fitted'=>'list<float>','marginal_residuals'=>'list<float>','aic'=>'float',
 'residual_variance'=>'float','sigma'=>'float','method'=>'string','converged'=>'bool'];
$group = ['group_labels'=>'list<int|string>','group_sizes'=>'list<int>','random_effects'=>'list<float>','random_effects_cond_var'=>'list<float>'];
$models=['Phit\fit_ols','Phit\fit_logistic','Phit\fit_probit','Phit\fit_poisson','Phit\fit_mixed_intercept','Phit\fit_mixed','Phit\fit_logistic_mixed'];
$families=['normal'=>'Normal(mean, sd)','t'=>"Student's t with df degrees of freedom",'chisquare'=>'Chi-square with df degrees of freedom',
 'f'=>'F with numerator df1 and denominator df2 degrees of freedom','beta'=>'Beta(alpha, beta) on [0, 1]',
 'gamma'=>'Gamma with shape and scale (mean shape*scale)','exponential'=>'Exponential with rate (mean 1/rate)',
 'lognormal'=>'Log-normal: ln X ~ Normal(meanlog, sdlog)','logistic'=>'Logistic(location, scale)',
 'binomial'=>'Binomial: successes in size trials with success probability prob','poisson'=>'Poisson with mean lambda',
 'nbinom'=>'Negative binomial: failures before the size-th success, success probability prob',
 'nbinom_mu'=>'Negative binomial with mean mu and size (variance mu + mu^2/size)'];
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
 'Phit\stat_approx'=>['summary'=>"Interpolate (x, y) at xout as R's approx(): method 'linear' or 'constant' (f weights the right-hand value),\n * rule 1 gives NAN outside [min(x), max(x)], rule 2 the nearest end value; tied x collapse by ties 'mean', 'min' or 'max'.",
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
 'Phit\fit_lm'=>['summary'=>"Linear model from a formula, like R's lm(): builds the model matrix and calls the fit_ols core (weights, offset() terms).\n * Factor levels unused after deletion are dropped, as lm() does. Rank deficiency raises 1004 instead of R's NA coefficients.",
  'result'=>$common+$olsKeys+$naming],
 'Phit\fit_glm'=>['summary'=>"Generalized linear model from a formula, like R's glm(): gaussian/identity (fit_ols core), binomial/logit, binomial/probit\n * and poisson/log (the matrix GLM cores). Binomial responses: 0/1, logical, factor/string (first level = failure),\n * proportions with trial weights, or cbind(successes, failures). The empty link selects the canonical link.",
  'result'=>$common+$glm+['family'=>'string','link'=>'string']+$naming+['t_statistics?'=>'list<float>','rss?'=>'float','residual_variance?'=>'float','sigma?'=>'float','r_squared?'=>'float','adj_r_squared?'=>'float']],
 'Phit\fit_lmer'=>['summary'=>"Gaussian mixed model from an lme4-style formula, like lme4::lmer(): fixed terms as in fit_lm and random bars\n * (x | g), (x || g), (1 | g1) + (1 | g2), (1 | g1:g2), (1 | g1/g2) mapped onto the fit_mixed core. REML by default.\n * Groups and effects follow first appearance after deletion, not lme4's sorted levels; terms keep formula order.
 * Each random term's effects_cond_var holds one k x k conditional covariance per group, as ranef(condVar = TRUE).",
  'result'=>$common+$mixed+['n_parameters'=>'int','bic'=>'float','theta'=>'list<float>','singular'=>'bool','optimizer'=>'string','optimizer_implementation'=>'string','evaluations'=>'int',
   'random'=>'list<array{covariance:list<list<float>>,effects:list<list<float>>,effects_cond_var:list<list<list<float>>>,group_labels:list<int|string>,group_sizes:list<int>,label:string,group:string,columns:list<string>,correlated:bool}>']+$naming],
 'Phit\fit_glmer'=>['summary'=>"Binomial random-intercept model from a formula, like lme4::glmer(family = binomial): exactly one (1 | g) term over the\n * fit_logistic_mixed core (Laplace by default, adaptive quadrature with quadraturePoints > 1). Responses as in fit_glm.
 * random_effects_cond_var holds each conditional mode's variance, as ranef(condVar = TRUE).",
  'result'=>$common+['z_statistics'=>'list<float>','fixed_fitted'=>'list<float>','linear_predictor'=>'list<float>','aic'=>'float','bic'=>'float','n_parameters'=>'int']+$group+
   ['n_groups'=>'int','random_intercept_sd'=>'float','random_intercept_variance'=>'float','neg2_log_likelihood'=>'float','quadrature_points'=>'int','integration'=>'string','method'=>'string','family'=>'string','link'=>'string','optimizer'=>'string','converged'=>'bool','evaluations'=>'int','gradient_max'=>'float','boundary'=>'bool','singular'=>'bool',
   'random'=>'list<array{label:string,group:string,columns:list<string>}>']+$naming],
];
$predict=['summary'=>"Predictions for new data from a fit_lm, fit_glm, fit_lmer or fit_glmer result, like R's predict():\n * the fit's formula is rebuilt over newdata with the fitted factor levels and poly() basis (its predvars key).\n * A level the fit never saw is an error. Rows with missing predictors predict null and are listed in missing_rows.\n * type 'response' applies the inverse link; seFit uses the delta method on that scale. Intervals ('confidence' or\n * 'prediction', t with df_residual) are for fit_lm fits. Mixed fits add random effects of known groups (random: false\n * gives population predictions; allowNewLevels predicts unseen groups at zero). GLM standard errors use the\n * covariance at the final coefficients (docs/r-compatibility.qmd).",
  'result'=>['fit'=>'list<float|null>','se_fit?'=>'list<float|null>','df?'=>'int','residual_scale?'=>'float','lower?'=>'list<float|null>','upper?'=>'list<float|null>','level'=>'float|null','n_input'=>'int','n_predicted'=>'int','missing_rows'=>'list<int>']];
$io=['Phit\read_csv'=>['summary'=>"Read a delimited text file the way phibble's PHP reader (after readr::read_delim()) does: same records,\n * names, guessed types, values and problem messages. Contract and phibble/readr differences: docs/io.qmd.",
  'options'=>"array{delimiter?: string, quote?: string, escape_double?: bool, escape_backslash?: bool, na?: list<string>, trim_ws?: bool,\n *   skip?: int, n_max?: ?int, comment?: string, skip_empty_rows?: bool, col_names?: bool, types?: array<int, '?'|'l'|'i'|'d'|'c'|'D'|'T'|'_'>,\n *   guess_max?: int, decimal_mark?: string, grouping_mark?: ?string}",
  'result'=>['names'=>'list<string>','columns'=>"list<null|array{type: 'bool'|'int'|'float'|'string'|'date'|'datetime', values: list<bool|int|float|string|null>, failed: int}>",'problems'=>'list<string>']]];
$text="<?php\n// Generated by scripts/php-stubs.php from the loaded native API.\n// IDE/static-analysis declarations only: do not require this file at runtime.\n// Statistical definitions and supported structures: docs/contracts.qmd.\n// Every function is in the Phit namespace: use function Phit\\fit_lm;\n\nnamespace Phit;\n\n";
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
 if (isset($draws[$name])) {
  $contract=$draws[$name];
  $api[$name]['result_keys']=array_keys($contract['result']);sort($api[$name]['result_keys']);
  $text.="/**\n * {$contract['summary']}\n * Rows are draws; columns are parameters. Preserves iteration order; no row deletion or intercept insertion.\n * null/NAN/INF and nonnumeric elements are rejected.\n";
  foreach($contract['params'] as $param=>$type)$text.=" * @param $type \$$param\n";
  $text.=" * @return array{";
  $fields=[];foreach($contract['result'] as $key=>$type)$fields[]="$key: $type";
  $text.=implode(', ',$fields)."}\n * @throws \\Exception Input/numerical error (codes 1001–1005).\n */\n";
 } elseif (isset($io[$name])) {
  $contract=$io[$name];
  $api[$name]['result_keys']=array_keys($contract['result']);sort($api[$name]['result_keys']);
  $text.="/**\n * {$contract['summary']}\n * Dates are int days since 1970-01-01; datetimes are int microseconds since the epoch (UTC); NA is null.\n * failed counts values that did not parse under the column's type (they are null). Skipped ('_') columns are null.\n";
  $text.=" * @param string \$path A readable file (bytes are passed through; no encoding validation)\n";
  $text.=" * @param {$contract['options']} \$options readr's defaults when omitted\n";
  $text.=" * @return array{";
  $fields=[];foreach($contract['result'] as $key=>$type)$fields[]="$key: $type";
  $text.=implode(', ',$fields)."}\n * @throws \\ValueError Missing or unreadable file, or an invalid option.\n */\n";
 } elseif (isset($formulas[$name])) {
  $contract=$formulas[$name];
  $api[$name]['result_keys']=array_map(fn($k)=>rtrim($k,'?'),array_keys($contract['result']));sort($api[$name]['result_keys']);
  $text.="/**\n * {$contract['summary']}\n * Formula syntax and R differences: docs/formulas.qmd. Rows with null/NAN in any variable the formula uses are\n * dropped with one E_WARNING (missing 'raise' rejects them); retained_rows/dropped_rows are original positions.\n";
  $text.=" * @param string \$formula R formula, e.g. 'y ~ log(x) + f * z + offset(log(t))'\n";
  $text.=" * @param array<string,list<int|float|bool|string|null>|array{levels:list<string>,codes:list<int|null>,ordered:bool}>|object \$data Named columns or factor descriptors; objects use toModelData(), else toColumns()\n";
  if($name==='Phit\fit_glm'||$name==='Phit\fit_glmer')$text.=" * @param string \$family 'gaussian', 'binomial' or 'poisson'\n * @param string \$link '' (canonical), 'identity', 'logit', 'probit' or 'log'\n";
  if($name!=='Phit\model_matrix')$text.=" * @param array<int|float|null>|string|null \$weights Positive prior weights aligned with the original rows, or a column name\n";
  $text.=" * @return array{\n";
  foreach($contract['result'] as $key=>$type)$text.=" *   $key: $type,\n";
  $text.=" * }\n * @throws \\Exception Statistical or input error (codes 1001–1008); unsupported functions/families/random terms raise 1007.\n */\n";
 } elseif ($name==='Phit\predict') {
  $api[$name]['result_keys']=array_map(fn($k)=>rtrim($k,'?'),array_keys($predict['result']));sort($api[$name]['result_keys']);
  $text.="/**\n * {$predict['summary']}\n";
  $text.=" * @param array<string,mixed> \$fit A formula fit result (needs formula, coefficients, covariance, columns and predvars)\n";
  $text.=" * @param array<string,list<int|float|bool|string|null>|array{levels:list<string>,codes:list<int|null>,ordered:bool}>|object \$newdata Named columns, as for the fit; the response may be absent\n";
  $text.=" * @param string \$type 'link' (linear predictor) or 'response'\n * @param string \$interval 'none', 'confidence' or 'prediction'\n * @param bool \$random Mixed fits: include random effects (lme4 re.form = NULL) or not (re.form = NA)\n";
  $text.=" * @return array{\n";
  foreach($predict['result'] as $key=>$type)$text.=" *   $key: $type,\n";
  $text.=" * }\n * @throws \\Exception Input error (1001), malformed fit, new levels, or unsupported options (1007).\n */\n";
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
   elseif ($p->getName()==='bw') $text.=" * @param string|float \$bw 'nrd0', 'nrd' or a positive bandwidth\n";
  }
  if ($smoothing[$name]['return']) $text.=" * @return {$smoothing[$name]['return']}\n";
  $text.=" * @throws \\Exception Input error (codes 1001–1003).\n */\n";
 } elseif (isset($descriptive[$name])) {
  $text.="/**\n * {$descriptive[$name]}\n * null/NAN are missing: missing 'raise' (default) rejects them, 'drop' removes them (pairwise for two arrays).\n";
  $text.=" * Nonfinite arithmetic raises 1003; constant-input correlation remains NAN.\n";
  foreach ($r->getParameters() as $p) if ((string)$p->getType()==='array') $text.=' * @param array<int|float|null> $'.$p->getName()."\n";
  if ($name==='Phit\stat_quantile_array') $text.=" * @return list<float>\n";
  $text.=" * @throws \\Exception Input error (codes 1001–1003).\n */\n";
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
  if ($name==='Phit\fit_ols') $keys+=['t_statistics'=>'list<float>','rss'=>'float','residual_variance'=>'float','sigma'=>'float','r_squared'=>'float','adj_r_squared'=>'float'];
  elseif (in_array($name,['Phit\fit_logistic','Phit\fit_probit','Phit\fit_poisson'],true)) $keys+=$glm;
  elseif ($name==='Phit\fit_mixed_intercept') $keys+=$mixed+$group+['evaluations'=>'int','n_groups'=>'int','random_intercept_variance'=>'float','icc'=>'float','boundary'=>'bool'];
  elseif ($name==='Phit\fit_mixed') $keys+=$mixed+['n_parameters'=>'int','bic'=>'float','theta'=>'list<float>','singular'=>'bool','optimizer'=>'string','optimizer_implementation'=>'string','evaluations'=>'int',
   'random'=>'list<array{covariance:list<list<float>>,effects:list<list<float>>,effects_cond_var:list<list<list<float>>>,group_labels:list<int|string>,group_sizes:list<int>}>'];
  else $keys+=['z_statistics'=>'list<float>','fixed_fitted'=>'list<float>','linear_predictor'=>'list<float>','aic'=>'float','bic'=>'float','n_parameters'=>'int']+$group+
   ['n_groups'=>'int','random_intercept_sd'=>'float','random_intercept_variance'=>'float','neg2_log_likelihood'=>'float','quadrature_points'=>'int','integration'=>'string','method'=>'string','family'=>'string','link'=>'string','optimizer'=>'string','converged'=>'bool','evaluations'=>'int','gradient_max'=>'float','boundary'=>'bool','singular'=>'bool'];
  $api[$name]['result_keys']=array_keys($keys);sort($api[$name]['result_keys']);
  $text.="/**\n * Predictor matrix: rows are observations. A fixed intercept is prepended by default.\n * null/NAN rows follow the missing policy; weights/offset are aligned with original rows.\n * @param array<array<int|float|null>> \$x\n * @param array<int|float|null> \$y\n * @param bool \$intercept False uses x exactly (including any caller-supplied ones).\n";
  if(in_array($name,['Phit\fit_mixed_intercept','Phit\fit_logistic_mixed'],true))$text.=" * @param array<int|string|null> \$groups\n";
  if($name==='Phit\fit_mixed')$text.=" * @param array<array{z:array<array<int|float|null>>,groups:array<int|string|null>,correlated?:bool}> \$random\n";
  $text.=" * @return array{\n";
  foreach($keys as $key=>$type)$text.=" *   $key: $type,\n";
  $text.=" * }\n * @throws \\Exception Statistical or input error (codes 1001–1008).\n */\n";
 }
 $text.='function '.substr($name,strlen('Phit\\')).'('.implode(', ',$params).'): '.$r->getReturnType()." {}\n\n";
}
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
