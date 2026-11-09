<?php
declare(strict_types=1);
function ivcheck(bool $ok, string $label): void { if (!$ok) throw new RuntimeException($label); }
function ivcompare(mixed $actual, mixed $expected, string $label): void {
    if (is_array($expected)) {
        ivcheck(!array_is_list($expected) || count($actual) === count($expected), "$label shape: ".count($actual).' != '.count($expected));
        foreach ($expected as $key => $value) ivcompare($actual[$key], $value, "$label/$key");
    } elseif ($expected === null || is_string($expected)) {
        ivcheck($actual === $expected, "$label mismatch");
    } else {
        ivcheck(is_numeric($actual) && abs($actual - $expected) <= 1e-9 + 1e-7 * abs($expected), "$label: ".var_export($actual,true)." != $expected");
    }
}
$fixtures = json_decode(file_get_contents(__DIR__.'/fixtures/iv-r.json'), true, flags: JSON_THROW_ON_ERROR);
set_error_handler(fn ($code, $message) => str_contains($message, 'dropped'));
foreach ($fixtures['cases'] as $case) {
    $fit = Phit\fit_iv($case['formula'], $case['data'], weights: $case['weights']);
    ivcheck($fit instanceof Phit\IvFit && $fit instanceof Phit\Fit, $case['name'].' fit class');
    foreach ($case['expected'] as $key => $expected) ivcompare($fit[$key], $expected, $case['name'].'/'.$key);
    ivcompare($fit['coefficients'], $case['fixest']['coefficients'], $case['name'].'/fixest coefficients');
    ivcompare($fit['std_errors'], $case['fixest']['std_errors'], $case['name'].'/fixest SE');
    $stageNumber = 0;
    foreach ($fit['first_stage'] as $name => $stage) {
        if ($case['name'] !== 'no_exog') {
            $ref = $case['ivreg_first_stage'][$stageNumber];
            ivcompare([$stage['df1'],$stage['df2'],$stage['f_statistic'],$stage['p_value']],$ref,$case['name'].'/ivreg first-stage F');
        }
        $stageNumber++;
        $canonical = function ($name) { $parts=explode(':',$name);sort($parts);return implode(':',$parts); };
        foreach ($case['fixest']['first_stage_f'] as $refName=>$reference) if ($case['name']!=='no_exog' && $canonical($refName)===$canonical($name)) ivcompare($stage['f_statistic'],$reference,$case['name'].'/fixest first-stage F');
        ivcheck(array_key_exists('sw_f_statistic', $stage), 'SW key always present');
        if (count($fit['endogenous']) === 1) ivcompare($stage['sw_f_statistic'], $stage['f_statistic'], 'single endogenous SW=F');
    }
    $newdata = $case['data'];
    foreach ($fit['instruments'] as $name) unset($newdata[$name]);
    // No response or instruments are required for structural prediction.
    unset($newdata['y']);
    $prediction = $fit->predict($newdata, seFit: true, interval: 'confidence');
    foreach ($fit['retained_rows'] as $j => $row) ivcompare($prediction['fit'][$row], $fit['fitted'][$j], $case['name'].' prediction');
    foreach (['se_fit'=>'se_fit','lower'=>'confidence_lower','upper'=>'confidence_upper'] as $key=>$reference) {
        $retainedValues = array_map(fn ($row) => $prediction[$key][$row], $fit['retained_rows']);
        ivcompare($retainedValues,$case['prediction'][$reference],$case['name'].'/'.$reference);
    }
    $interval = $fit->predict($newdata, interval:'prediction');
    foreach (['lower'=>'prediction_lower','upper'=>'prediction_upper'] as $key=>$reference) {
        $retainedValues = array_map(fn ($row) => $interval[$key][$row], $fit['retained_rows']);
        ivcompare($retainedValues,$case['prediction'][$reference],$case['name'].'/'.$reference);
    }
    $summary = $fit->summary();
    ivcheck(str_contains($summary,'two-stage least squares') && str_contains($summary,'Sanderson-Windmeijer') && str_contains($summary,'Wu-Hausman'), 'IV summary');
    ivcheck($fit->tidy()['term'] === $fit['columns'], 'IV tidy');
    $scaled = $case['weights'] === null ? array_fill(0,count($case['data']['y']),7.0) : array_map(fn ($v) => $v === null ? null : 7*$v, $case['data']['w']);
    $scaledFit = Phit\fit_iv($case['formula'], $case['data'], weights: $scaled);
    ivcompare($scaledFit['coefficients'], $fit['coefficients'], 'weight scale coefficients');
    ivcompare($scaledFit['covariance'], $fit['covariance'], 'weight scale covariance');
    ivcompare($scaledFit['first_stage'], $fit['first_stage'], 'weight scale diagnostics');
}
restore_error_handler();
$data = $fixtures['cases'][0]['data'];
foreach ([['y ~ x | e1 + e2 ~ z1',1004],['y ~ x | e1 ~ z1 + I(z1)',1004],['y ~ x + (1 | g) | e1 ~ z1',1007],['y ~ x | e1 ~ z1 + (1 | g)',1007],['y ~ x | e1 ~ z1 | z2',1001],['y ~ x',1001],['y ~ x | e1 - x ~ z1 + z2',1001]] as [$formula,$code]) {
    try { Phit\fit_iv($formula,$data); throw new RuntimeException('expected error '.$formula); }
    catch (Exception $e) { ivcheck($e->getCode()===$code,"$formula error {$e->getCode()} != $code"); }
}
try { Phit\fit_iv('y ~ x | e1 ~ z1',$data,vcov:'HC1'); throw new RuntimeException('vcov error'); } catch (Exception $e) { ivcheck($e->getCode()===1007,'vcov unsupported'); }
try { Phit\fit_ols('y ~ x | e1 ~ z1',$data); throw new RuntimeException('OLS IV rejection'); } catch (Exception $e) { ivcheck($e->getCode()!==0,'OLS IV rejection'); }
$base = Phit\fit_iv('y ~ x | e1 ~ z1 + z2', $data);
ivcompare(Phit\fit_iv('y ~ x | e1 ~ 0 + z1 + z2', $data)['coefficients'], $base['coefficients'], 'excluded intercept flag preserves exogenous intercept');
ivcompare(Phit\fit_iv('y ~ x | e1 ~ z1 + z2 - x', $data)['coefficients'], $base['coefficients'], 'excluded subtraction preserves required exogenous instruments');
$quoted = $data; $quoted['e~a|b'] = $quoted['e1'];
ivcompare(Phit\fit_iv('y ~ x | `e~a|b` ~ z1 + z2', $quoted)['coefficients'], $base['coefficients'], 'quoted delimiters');
$noExog = Phit\fit_iv('y ~ 0 | e1 ~ z1 + z2', $data);
ivcheck(count($noExog['coefficients']) === 1 && !$noExog['intercept'], 'one endogenous without exogenous columns');
$degenerate = ['a'=>[1,0,0,0,0], 'b'=>[0,1,0,0,0], 'c'=>[0,0,1,0,0], 'y'=>[1,2,3,4,5]];
$nullSw = Phit\fit_iv('y ~ 0 | a + b ~ a + b + c', $degenerate);
foreach ($nullSw['first_stage'] as $stage) ivcheck(array_key_exists('sw_f_statistic',$stage) && $stage['sw_f_statistic'] === null, 'undefined SW variance is explicit null');
ivcheck(str_contains(implode(' ', $nullSw['diagnostics']['warnings']), 'Sanderson-Windmeijer'), 'undefined SW warns');
echo "IV reference and contract checks passed\n";
