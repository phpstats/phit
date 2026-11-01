<?php
declare(strict_types=1);
function check_stability(bool $ok,string $label):void{if(!$ok)throw new RuntimeException($label);}
function compare_stability(mixed $a,mixed $b,float $abs,float $rel,string $label):void{
 if(is_array($b)){check_stability(count($a)===count($b),"$label shape");foreach($b as $k=>$v)compare_stability($a[$k],$v,$abs,$rel,"$label/$k");}
 else check_stability(is_finite((float)$a)&&abs($a-$b)<=$abs+$rel*abs($b),"$label: $a != $b");
}
$fixture=json_decode(file_get_contents(__DIR__.'/fixtures/stress-r.json'),true,flags:JSON_THROW_ON_ERROR);
foreach($fixture['cases'] as $c){
 $function='Phit\fit_'.$c['model'].'_matrix';$r=$function($c['x'],$c['y'],weights:$c['weights'],offset:$c['offset'], intercept: false);
 foreach($c['expected'] as $key=>$expected){
  $actualKey=$key==='statistics'?($c['model']==='ols'?'t_statistics':'z_statistics'):$key;
  $factor=match($key){'covariance'=>$c['response_scale']**2,'coefficients','std_errors','conf_int','fitted','residuals'=>$c['response_scale'],default=>1};
  compare_stability($r[$actualKey],$expected,$c['tolerance']['absolute']*$factor,$c['tolerance']['relative'],$c['name'].'/'.$key);
 }
 check_stability($r['diagnostics']['converged']&&$r['diagnostics']['warnings']===[],'successful diagnostics');
}
$errors=[
 [fn()=>Phit\fit_ols_matrix([[1,1],[1,2],[1,3],[1,4]],[1e155,2e155,3e155,4e155],weights:[1,2,3,4], intercept: false),1003],
 [fn()=>Phit\fit_ols_matrix([[1],[1],[1]],['oops',2,3], intercept: false),1001],
 [fn()=>Phit\fit_ols_matrix([[1],[1],[1]],[1], intercept: false),1002],
 [fn()=>Phit\fit_ols_matrix([[1],[1],[1]],[1,INF,3], intercept: false),1003],
 [fn()=>Phit\fit_ols_matrix([[1,1],[1,1],[1,1]],[1,2,3], intercept: false),1004],
 [fn()=>Phit\fit_logistic_matrix([[1,-2],[1,-1],[1,1],[1,2]],[0,0,1,1], intercept: false),1006],
 [fn()=>Phit\fit_logistic_matrix(array_fill(0,4,[1]),[0,1,0,1],weights:array_fill(0,4,1000001), intercept: false),1007],
 [fn()=>Phit\fit_ols_matrix(array_fill(0,4,[1]),[1e-170,2e-170,3e-170,4e-170], intercept: false),1003],
];
// Validate original input domains before deletion; all entry points share codes.
foreach(['Phit\fit_logistic_matrix','Phit\fit_probit_matrix','Phit\fit_mixed_logistic_matrix'] as $function){
 foreach([-0.1,1.1] as $bad){
  $args=['x'=>[[null],[1],[1],[1]],'y'=>[$bad,1,0,1]];
  if($function==='Phit\fit_mixed_logistic_matrix')$args['groups']=[1,1,2,2];
  $errors[]=[fn()=>$function(...$args, intercept: false),1001];
 }
}
foreach([-1,0.5] as $bad)$errors[]=[fn()=>Phit\fit_poisson_matrix([[null],[1],[1],[1]],[$bad,1,2,3], intercept: false),1001];
foreach(['Phit\fit_mixed_gaussian_intercept_matrix','Phit\fit_mixed_logistic_matrix'] as $function){
 foreach([true,1.0,[]] as $bad){
  $errors[]=[fn()=>$function([[null],[1],[1],[1]],[0,1,0,1],[$bad,1,2,2], intercept: false),1001];
 }
}
$errors[]=[fn()=>Phit\fit_mixed_gaussian_matrix([[null],[1],[1],[1]],[0,1,0,1],
 [['z'=>[[1],[1],[1],[1]],'groups'=>[true,1,2,2]]], intercept: false),1001];
foreach($errors as [$call,$code]){try{$call();throw new RuntimeException('expected a statistical exception');}catch(Exception $e){check_stability($e->getCode()===$code,"exception category $code: ".$e->getMessage());}}
foreach(['Phit\fit_logistic_matrix','Phit\fit_probit_matrix'] as $fn){
 $x=array_fill(0,8,[1]);$y=[0,1,0,1,0,1,0,1];$near=array_map(fn($v)=>$v===0?1e-10:1-1e-10,$y);
 check_stability($fn($x,$y, intercept: false)['coefficients']===$fn($x,$near, intercept: false)['coefficients'],'count canonicalization');
 try{$fn($x,array_fill(0,8,1e-10), intercept: false);throw new RuntimeException('fictitious events');}catch(Exception $e){check_stability($e->getCode()===1001,'both classes required');}
}
$schema=json_decode(file_get_contents(__DIR__.'/fixtures/php-api.json'),true,flags:JSON_THROW_ON_ERROR);
$cases=[
 'Phit\fit_ols_matrix'=>['ols-r.json',fn($c)=>Phit\fit_ols_matrix($c['x'],$c['y'], intercept: false)],
 'Phit\fit_logistic_matrix'=>['logistic-r.json',fn($c)=>Phit\fit_logistic_matrix($c['x'],$c['y'], intercept: false)],
 'Phit\fit_probit_matrix'=>['probit-r.json',fn($c)=>Phit\fit_probit_matrix($c['x'],$c['y'], intercept: false)],
 'Phit\fit_poisson_matrix'=>['poisson-r.json',fn($c)=>Phit\fit_poisson_matrix($c['x'],$c['y'], intercept: false)],
 'Phit\fit_mixed_gaussian_intercept_matrix'=>['lmm-r.json',fn($c)=>Phit\fit_mixed_gaussian_intercept_matrix($c['x'],$c['y'],$c['groups'], intercept: false)],
 'Phit\fit_mixed_gaussian_matrix'=>['lmm-expanded-r.json',fn($c)=>Phit\fit_mixed_gaussian_matrix($c['x'],$c['y'],$c['terms'],reml:$c['reml'],weights:$c['weights'],offset:$c['offset'], intercept: false)],
 'Phit\fit_mixed_logistic_matrix'=>['logistic-mixed-r.json',fn($c)=>Phit\fit_mixed_logistic_matrix($c['x'],$c['y'],$c['groups'],weights:$c['weights'],offset:$c['offset'], intercept: false)],
];
foreach($cases as $name=>[$file,$call]){
 $c=json_decode(file_get_contents(__DIR__.'/fixtures/'.$file),true,flags:JSON_THROW_ON_ERROR)['cases'][0];$r=$call($c);
 $keys=array_keys($r);sort($keys);check_stability($keys===$schema[$name]['result_keys'],"$name result/stub keys");
 check_stability($r['diagnostics']['converged']&&is_array($r['diagnostics']['warnings']),"$name diagnostics");
 foreach(['iterations','evaluations','gradient_max','boundary','singular'] as $key)if(array_key_exists($key,$r))check_stability($r['diagnostics'][$key]===$r[$key],"$name/$key diagnostics alias");
}
$warnings=[];set_error_handler(function($severity,$message)use(&$warnings){$warnings[]=$message;return true;});
try{$r=Phit\fit_ols_matrix([[1],[1],[1],[null]],[1,3,2,4], intercept: false);}finally{restore_error_handler();}
check_stability(count($warnings)===1&&count($r['diagnostics']['warnings'])===1&&$r['dropped_rows']===[3],'listwise diagnostics');
echo "PHP stability, result contracts and exception categories passed\n";
