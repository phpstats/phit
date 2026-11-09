<?php
declare(strict_types=1);
function ridge_check(bool $ok,string $label): void { if (!$ok) throw new RuntimeException($label); }
function ridge_compare(mixed $actual,mixed $expected,string $label): void {
    if (is_array($expected)) {
        ridge_check(is_array($actual) && (!array_is_list($expected) || count($actual)===count($expected)), "$label shape");
        foreach ($expected as $key=>$value) ridge_compare($actual[$key],$value,"$label/$key");
    } elseif ($expected===null || is_string($expected) || is_bool($expected)) ridge_check($actual===$expected,"$label mismatch ".var_export($actual,true));
    else ridge_check(is_numeric($actual) && abs($actual-$expected)<=1e-7+2e-6*abs($expected),"$label: ".var_export($actual,true)." != $expected");
}
$fixtures=json_decode(file_get_contents(__DIR__.'/fixtures/ridge-r.json'),true,flags:JSON_THROW_ON_ERROR);
set_error_handler(fn($code,$message)=>str_contains($message,'dropped'));
foreach ($fixtures['fits'] as $case) {
    $fit=Phit\fit_ridge($case['formula'],$case['data'],lambda:$case['lambda'],penaltyFactor:$case['penaltyFactor'],standardize:$case['standardize'],bounds:$case['bounds'],weights:$case['weights']);
    ridge_check($fit instanceof Phit\RidgeFit && $fit instanceof Phit\Fit,'RidgeFit class');
    foreach ($case['expected'] as $key=>$expected) ridge_compare($fit[$key],$expected,$case['name'].'/'.$key);
    if ($case['MASS']!==null) ridge_compare($fit['coefficients'],$case['MASS'],$case['name'].'/MASS');
    ridge_check(str_contains($fit->summary(),'not coverage-valid'),'conditional inference caveat');
    ridge_check($fit->tidy()['term']===$fit['columns'],'ridge tidy');
    $prediction=$fit->predict($case['data'],seFit:true,interval:'confidence');
    foreach ($fit['retained_rows'] as $i=>$row) ridge_compare($prediction['fit'][$row],$fit['fitted'][$i],$case['name'].'/predict');
    foreach ($fit['at_bound'] as $j=>$atBound) if ($atBound) ridge_check($fit['std_errors'][$j]===null && $fit['conf_int'][$j]===null,'at-bound uncertainty null');
    $scaled=array_map(fn($w)=>$w===null?null:7*$w,$case['weights']===null?array_fill(0,count($case['data']['y']),1.):$case['data']['w']);
    $other=Phit\fit_ridge($case['formula'],$case['data'],$case['lambda'],penaltyFactor:$case['penaltyFactor'],standardize:$case['standardize'],bounds:$case['bounds'],weights:$scaled);
    ridge_compare($other['coefficients'],$fit['coefficients'],'weight invariance coefficients');
    ridge_compare($other['covariance'],$fit['covariance'],'weight invariance covariance');
}
$lmr = $fixtures['lmridge'];
$referenceFit = Phit\fit_ridge($fixtures['fits'][0]['formula'],$fixtures['fits'][0]['data'],$fixtures['fits'][0]['lambda']);
ridge_compare($referenceFit['coefficients'],$lmr['coefficients'],'lmridge coefficients');
for ($i=0;$i<3;$i++) for ($j=0;$j<3;$j++) ridge_compare($referenceFit['covariance'][$i+1][$j+1]/$referenceFit['sigma']**2,$lmr['covariance'][$i][$j]/$lmr['sigma2'],'lmridge conditional covariance per sigma squared');
foreach ($fixtures['cv'] as $case) {
    $fit=Phit\cv_ridge($case['formula'],$case['data'],lambda:$case['lambda'],foldid:$case['foldid'],penaltyFactor:$case['penaltyFactor'],standardize:$case['standardize'],bounds:$case['bounds'],weights:$case['weights']);
    foreach ($case['expected_cv'] as $key=>$expected) ridge_compare($fit[$key],$expected,$case['cv_name'].'/'.$key);
    ridge_check($fit['fit'] instanceof Phit\RidgeFit,'CV nested fit');
    ridge_compare($fit['fit']['lambda'],$fit['lambda_min'],'CV selected fit');
}
restore_error_handler();
$data=$fixtures['fits'][0]['data'];
foreach ([fn()=>Phit\fit_ridge('y ~ a + b',$data,-1),fn()=>Phit\fit_ridge('y ~ a + b',$data,1,penaltyFactor:['typo*'=>1]),fn()=>Phit\fit_ridge('y ~ a + b',$data,1,bounds:['a'=>[2,1]]),fn()=>Phit\cv_ridge('y ~ a + b',$data,lambda:[.1,1]),fn()=>Phit\cv_ridge('y ~ a + b',$data,nfolds:2)] as $call) {
    try {$call();throw new RuntimeException('expected ridge error');}catch(Exception $e){ridge_check($e->getCode()===1001,'ridge input error '.$e->getMessage());}
}
$fixed=Phit\fit_ridge('y ~ a + b + c',$data,1,bounds:['a'=>[.5,.5]]);
ridge_compare($fixed['named']['a'],.5,'equality bound');
try {$fixed->predict($data,interval:'prediction');throw new RuntimeException('expected interval rejection');}catch(Exception $e){ridge_check($e->getCode()===1007,'ridge prediction interval unsupported');}
$zero=Phit\fit_ridge('y ~ a + b + c',$data,0);
$ols=Phit\fit_ols('y ~ a + b + c',$data);
ridge_compare($zero['coefficients'],$ols['coefficients'],'lambda zero OLS');
$seeded=Phit\cv_ridge('y ~ a + b + c',$data,lambda:[1,.1],nfolds:4,seed:9);
ridge_compare($seeded['cvm'],Phit\cv_ridge('y ~ a + b + c',$data,lambda:[1,.1],nfolds:4,seed:9)['cvm'],'seed reproducibility');
$prediction=$referenceFit->predict($data,seFit:true,interval:'confidence');
for ($row=0;$row<5;$row++) {
    $x=[1.,$data['a'][$row],$data['b'][$row],$data['c'][$row]];
    $variance=0.;
    for ($i=0;$i<4;$i++) for ($j=0;$j<4;$j++) $variance+=$x[$i]*$fixtures['fits'][0]['expected']['covariance'][$i][$j]*$x[$j];
    ridge_compare($prediction['se_fit'][$row],sqrt($variance),'conditional prediction SE');
}
ridge_check($fixed->predict($data,seFit:true)['se_fit'][0]===null,'at-bound prediction uncertainty');
$default=Phit\cv_ridge('y ~ a + b + c',$data,nfolds:3,seed:4);
ridge_check(count($default['lambda'])===100 && $default['lambda'][0]>$default['lambda'][99],'default path');
ridge_compare($default['lambda'][0],100*$default['fit']['response_scale'],'default lambda upper');
$constant=$data;$constant['constant']=array_fill(0,count($data['y']),2.);
$constantFit=Phit\fit_ridge('y ~ a + b + c + constant',$constant,1);
ridge_compare($constantFit['named']['constant'],0.,'constant column');
$collinear=$data;$collinear['duplicate']=$data['a'];
$collinearFit=Phit\fit_ridge('y ~ a + duplicate',$collinear,1);
ridge_compare($collinearFit['named']['a'],$collinearFit['named']['duplicate'],'penalized collinearity');
try {Phit\fit_ridge('y ~ a + duplicate',$collinear,0);throw new RuntimeException('expected rank rejection');}catch(Exception $e){ridge_check($e->getCode()===1004,'lambda zero rank error');}
echo "Ridge and CV glmnet reference checks passed\n";
