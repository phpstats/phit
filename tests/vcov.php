<?php
declare(strict_types=1);
function vcov_check(bool $ok,string $label): void {if(!$ok)throw new RuntimeException($label);}
function vcov_compare(mixed $a,mixed $b,string $path): void {
 if(is_array($b)){vcov_check(is_array($a)&&(!array_is_list($b)||count($a)===count($b)),"$path shape");foreach($b as $k=>$v)vcov_compare($a[$k],$v,"$path/$k");}
 elseif($b===null||is_string($b)||is_bool($b))vcov_check($a===$b,"$path mismatch");
 else vcov_check(is_numeric($a)&&abs($a-$b)<=1e-9+1e-7*abs($b),"$path: ".var_export($a,true)." != $b");
}
$fixtures=json_decode(file_get_contents(__DIR__.'/fixtures/vcov-r.json'),true,flags:JSON_THROW_ON_ERROR);
set_error_handler(fn($code,$message)=>str_contains($message,'dropped'));
foreach($fixtures['cases'] as $case){
 $data=$fixtures['datasets'][$case['dataset']];$call='Phit\\'.$case['fun'];
 $fit=$call($case['formula'],$data,weights:$case['weights'],vcov:$case['vcov']);
 foreach($case['expected'] as $key=>$expected)vcov_compare($fit[$key],$expected,$case['name'].'/'.$key);
 $predict=$fit->predict($data,seFit:true,interval:'confidence');
 vcov_compare($predict['df'],$fit['inference_df'],'prediction df');
 foreach($case['prediction'] as $key=>$expected)foreach($fit['retained_rows'] as $j=>$i)vcov_compare($predict[$key][$i],$expected[$j],$case['name'].'/predict/'.$key.'/'.$i);
 $tidy=$fit->tidy(confInt:true);
 foreach($fit['coefficients'] as $j=>$b){vcov_compare($tidy['p_value'][$j],$fit['p_values'][$j],'tidy p');vcov_compare([$tidy['conf_low'][$j],$tidy['conf_high'][$j]],$case['expected']['conf_int'][$j],'tidy CI');}
 vcov_check(str_contains($fit->summary(),'Covariance: '.$fit['vcov_label']),'summary covariance');
 vcov_check(!str_contains($fit->summary(),'F-statistic:'),'omit classical overall F with robust covariance');
 $weights=array_fill(0,count($data['y']),null);
 foreach($fit['retained_rows'] as $i)$weights[$i]=$case['weights']===null?1.:$data[$case['weights']][$i];
 $iid=$call($case['formula'],$data,weights:$weights);
 foreach(['coefficients','fitted','residuals','sigma','rss','df_residual'] as $key)vcov_compare($fit[$key],$iid[$key],'unchanged '.$key);
 if($case['fun']==='fit_iv') {vcov_compare($fit['first_stage'],$iid['first_stage'],'unchanged first stage');vcov_compare($fit['diagnostics'],$iid['diagnostics'],'unchanged diagnostics');}
 if(is_array($case['vcov'])&&isset($case['vcov']['cluster'])){
  $arrayOption=$case['vcov'];$arrayOption['cluster']=$data['day'];
  vcov_compare($call($case['formula'],$data,weights:$case['weights'],vcov:$arrayOption)['covariance'],$fit['covariance'],'original-row cluster arrays');
 }
}
restore_error_handler();
$data=$fixtures['datasets']['base'];$n=count($data['y']);
$hc0=Phit\fit_ols('y ~ x + d',$data,vcov:'HC0');
vcov_compare(Phit\fit_ols('y ~ x + d',$data,vcov:['newey_west'=>0])['covariance'],$hc0['covariance'],'NW0 equals HC0');
vcov_compare(Phit\fit_ols('y ~ x + d',$data,vcov:['newey_west'=>0,'adjust'=>true])['covariance'],Phit\fit_ols('y ~ x + d',$data,vcov:'HC1')['covariance'],'NW0 adjusted equals HC1');
$precise=array_merge(...array_fill(0,$n/4,[1.,1.000000000000001,2.,2.000000000000001]));
$groups=array_merge(...array_fill(0,$n/4,[0,1,2,3]));
$cluster=Phit\fit_ols('y ~ x + d',$data,vcov:['cluster'=>$groups]);
vcov_compare(Phit\fit_ols('y ~ x + d',$data,vcov:['cluster'=>$precise])['covariance'],$cluster['covariance'],'lossless floating cluster labels');
$large=array_merge(...array_fill(0,$n/4,[PHP_INT_MAX,PHP_INT_MAX-1,PHP_INT_MAX-2,PHP_INT_MAX-3]));
vcov_compare(Phit\fit_ols('y ~ x + d',$data,vcov:['cluster'=>$large])['covariance'],$cluster['covariance'],'lossless integer cluster labels');
$mixed=array_merge(...array_fill(0,$n/4,[9223372036854774784,(float)9223372036854774784,1,2]));
$three=array_merge(...array_fill(0,$n/4,[0,0,1,2]));
vcov_compare(Phit\fit_ols('y ~ x + d',$data,vcov:['cluster'=>$mixed])['covariance'],Phit\fit_ols('y ~ x + d',$data,vcov:['cluster'=>$three])['covariance'],'exact integral float label normalization');
foreach(['HC4',[],['cluster'=>array_fill(0,$n,1)],['cluster'=>'day','type'=>'HC3'],['newey_west'=>-1],['newey_west'=>$n],['newey_west'=>1,'adjust'=>1],['cluster'=>'day','newey_west'=>1]] as $option){
 try{Phit\fit_ols('y ~ x + d',$data,vcov:$option);throw new RuntimeException('expected covariance error');}catch(Exception $e){vcov_check($e->getCode()===1001,'covariance validation '.$e->getMessage());}
}
try{Phit\fit_ols('y ~ x + d',$data,vcov:null);throw new RuntimeException('expected null rejection');}catch(TypeError $e){}
$weighted=Phit\fit_iv('y ~ x | d ~ z1 + z2 + z3',$data,weights:'w',vcov:'HC3');
vcov_compare(Phit\fit_iv('y ~ x | d ~ z1 + z2 + z3',$data,weights:array_map(fn($w)=>7*$w,$data['w']),vcov:'HC3')['covariance'],$weighted['covariance'],'robust weight scaling invariance');
echo count($fixtures['cases'])." sandwich covariance cases, inference, predictions, and edge checks passed\n";
