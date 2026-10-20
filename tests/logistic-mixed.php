<?php
declare(strict_types=1);
function bcheck(bool $ok,string $label):void {if(!$ok)throw new RuntimeException($label);}
function bcompare(mixed $a,mixed $b,string $label,float $abs,float $rel):void {
 if(is_array($b)){bcheck(count($a)===count($b),"$label shape");foreach($b as $k=>$v)bcompare($a[$k],$v,"$label/$k",$abs,$rel);}
 elseif(is_bool($b)){bcheck($a===$b,"$label bool");}
 else {bcheck(is_finite((float)$a)&&abs($a-$b)<=$abs+$rel*abs($b),"$label: $a != $b");}
}
$f=json_decode(file_get_contents(__DIR__.'/fixtures/logistic-mixed-r.json'),true,flags:JSON_THROW_ON_ERROR);
foreach($f['cases'] as $c){
 $r=Phit\fit_logistic_mixed($c['x'],$c['y'],$c['groups'],weights:$c['weights'],offset:$c['offset'],quadraturePoints:$c['quadrature_points'], intercept: false);
 foreach($c['expected'] as $k=>$v){
  [$abs,$rel]=in_array($k,['covariance','std_errors','z_statistics','conf_int','p_values'],true)?[2e-5,2e-3]:
   (in_array($k,['log_likelihood','neg2_log_likelihood','aic','bic'],true)?[1e-6,1e-8]:[2e-5,2e-4]);
  bcompare($r[$k],$v,$c['name'].'/'.$k,$abs,$rel);
 }
 bcheck($r['n_parameters']===count($c['x'][0])+1&&$r['df_residual']===count($c['y'])-$r['n_parameters'],'parameter counts');
 bcheck($r['group_labels']===array_values(array_unique($c['groups'])),'first appearance');
 bcheck($r['method']==='ML'&&$r['family']==='binomial'&&$r['link']==='logit'&&$r['converged'],'fit metadata');
 bcheck($r['integration']===($c['quadrature_points']===1?'laplace':'adaptive_gauss_hermite'),'integration metadata');
}
$c=array_values(array_filter($f['cases'],fn($c)=>$c['name']==='grouped_offset_1'))[0];$c['groups']=array_map(fn($g)=>'school-'.$g,$c['groups']);
$c['x'][2][1]=null;$c['y'][4]=NAN;$c['groups'][6]=null;$c['weights'][8]=null;$c['offset'][10]=NAN;
$warnings=[];set_error_handler(function($severity,$message)use(&$warnings){$warnings[]=[$severity,$message];return true;});
try{$r=Phit\fit_logistic_mixed($c['x'],$c['y'],$c['groups'],weights:$c['weights'],offset:$c['offset'],quadraturePoints:9, intercept: false);}finally{restore_error_handler();}
bcheck($r['dropped_rows']===[2,4,6,8,10]&&count($warnings)===1&&$warnings[0][0]===E_WARNING,'union deletion warning');
$select=fn($v)=>array_values(array_filter($v,fn($i)=>!in_array($i,[2,4,6,8,10],true),ARRAY_FILTER_USE_KEY));
$complete=Phit\fit_logistic_mixed($select($c['x']),$select($c['y']),$select($c['groups']),weights:$select($c['weights']),offset:$select($c['offset']),quadraturePoints:9, intercept: false);
bcompare($r['fitted'],$complete['fitted'],'retained alignment',1e-10,1e-10);
foreach([['missing'=>'raise'],['missing'=>'bad'],['quadraturePoints'=>0],['quadraturePoints'=>62],['maxEvaluations'=>4],['tolerance'=>NAN],
 ['groups'=>array_fill(0,count($c['y']),1)],['groups'=>array_fill(0,count($c['y']),1.0)],['weights'=>[1]],['offset'=>[0]],
 ['weights'=>array_fill(0,count($c['y']),0.5)],['weights'=>array_fill(0,count($c['y']),1000001)],['y'=>array_fill(0,count($c['y']),1)],
] as $options){$thrown=false;try{Phit\fit_logistic_mixed(...array_replace(['x'=>$c['x'],'y'=>$c['y'],'groups'=>$c['groups'],'weights'=>$c['weights'],'offset'=>$c['offset']],$options), intercept: false);}catch(Throwable $e){$thrown=true;}bcheck($thrown,'invalid logistic mixed inputs');}
// Invalid values are errors even on a row that would be dropped.
$bad=$c['x'];$bad[2][0]=INF;$thrown=false;try{Phit\fit_logistic_mixed($bad,$c['y'],$c['groups'],weights:$c['weights'],offset:$c['offset'], intercept: false);}catch(Throwable $e){$thrown=true;}bcheck($thrown,'validation before deletion');
$r=Phit\fit_logistic_mixed(array_fill(0,8,[1]),[0,1,0,1,0,1,0,1],[1,1,'1','1','A','A','B','B'], intercept: false);
bcheck($r['group_labels']===[1,'1','A','B']&&$r['boundary'],'typed IDs and exact boundary');
echo "PHP binomial random-intercept integration tests passed (28 lme4 fits)\n";
