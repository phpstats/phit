<?php
declare(strict_types=1);
require __DIR__.'/r-compatibility.php';
// Reference terms: (x || g) keeps only conditional-variance diagonals, as lme4 reports them.
function expanded_terms(array $random,array $input):array{
 return array_map(function($t,$in){
  $out=['covariance'=>$t['covariance'],'effects'=>$t['effects']];
  if($in['correlated']??true)$out['effects_cond_var']=$t['effects_cond_var'];
  else $out['effects_cond_var_diagonal']=array_map(fn($b)=>array_map(fn($j)=>$b[$j][$j],array_keys($b)),$t['effects_cond_var']);
  return $out;
 },$random,$input);
}
$f=json_decode(file_get_contents(__DIR__.'/fixtures/lmm-expanded-r.json'),true,flags:JSON_THROW_ON_ERROR);
foreach($f['cases'] as $c){
 $r=Phit\fit_mixed(x:$c['x'],y:$c['y'],random:$c['terms'],reml:$c['reml'],weights:$c['weights'],offset:$c['offset'], intercept: false);
 $r['terms']=expanded_terms($r['random'],$c['terms']);
 foreach($c['references'] as $optimizer=>$ref)foreach($ref['result'] as $k=>$v){
  [$abs,$rel]=in_array($k,['log_likelihood','aic','bic'],true)?[5e-6,1e-9]:($optimizer==='bobyqa'?[1e-5,1e-5]:[1e-3,3e-4]);
  compat_compare($r[$k],$v,$c['name'].'/'.$optimizer.'/'.$k,$abs,$rel);
 }
 $nm=Phit\fit_mixed($c['x'],$c['y'],$c['terms'],reml:$c['reml'],weights:$c['weights'],offset:$c['offset'],optimizer:'nelder_mead', intercept: false);
 $nm['terms']=expanded_terms($nm['random'],$c['terms']);
 foreach($c['references']['Nelder_Mead']['result'] as $k=>$v){
  [$abs,$rel]=in_array($k,['log_likelihood','aic','bic'],true)?[5e-6,1e-9]:[1e-3,3e-4];
  compat_compare($nm[$k],$v,$c['name'].'/native_Nelder_Mead/'.$k,$abs,$rel);
 }
 compat_check($nm['optimizer']==='nelder_mead'&&$nm['converged'],'native simplex metadata');
 compat_check($r['method']===($c['reml']?'REML':'ML')&&$r['optimizer']==='bobyqa'&&$r['converged'],'expanded fit metadata');
 foreach($c['terms'] as $i=>$term)compat_check($r['random'][$i]['group_labels']===array_values(array_unique($term['groups'])),'term group order');
}
$c=$f['cases'][3];$c['terms'][0]['groups']=array_map(fn($g)=>'school-'.$g,$c['terms'][0]['groups']);
$c['terms'][1]['groups']=array_map(fn($g)=>'teacher-'.$g,$c['terms'][1]['groups']);
$c['terms'][0]['z'][1][1]=null;$c['terms'][1]['groups'][3]=null;$c['weights'][5]=NAN;$c['offset'][7]=null;
$warnings=[];set_error_handler(function($severity,$message)use(&$warnings){$warnings[]=[$severity,$message];return true;});
try{$r=Phit\fit_mixed($c['x'],$c['y'],$c['terms'],weights:$c['weights'],offset:$c['offset'], intercept: false);}finally{restore_error_handler();}
compat_check($r['dropped_rows']===[1,3,5,7]&&count($warnings)===1&&$warnings[0][0]===E_WARNING,'mixed union deletion');
$select=fn($v)=>array_values(array_filter($v,fn($i)=>!in_array($i,[1,3,5,7],true),ARRAY_FILTER_USE_KEY));
$terms=array_map(fn($t)=>['z'=>$select($t['z']),'groups'=>$select($t['groups']),'correlated'=>$t['correlated']],$c['terms']);
$complete=Phit\fit_mixed($select($c['x']),$select($c['y']),$terms,weights:$select($c['weights']),offset:$select($c['offset']), intercept: false);
compat_compare($r['fitted'],$complete['fitted'],'all retained rows aligned');
foreach([
 ['random'=>[]],['random'=>[['z'=>[[1]],'groups'=>[1]]]],['random'=>[['z'=>array_fill(0,count($c['y']),[1]),'groups'=>array_fill(0,count($c['y']),true)]]],
 ['random'=>[['z'=>array_fill(0,count($c['y']),[1]),'groups'=>array_fill(0,count($c['y']),1),'correlated'=>'yes']]],
 ['optimizer'=>'nloptwrap'],['objectiveTolerance'=>NAN],['missing'=>'raise'],['tolerance'=>NAN],['start'=>[-1]],['maxEvaluations'=>-1],['maxEvaluations'=>4],
] as $options){$thrown=false;try{Phit\fit_mixed(...array_replace(['x'=>$c['x'],'y'=>$c['y'],'random'=>$c['terms'],'weights'=>$c['weights'],'offset'=>$c['offset']],$options), intercept: false);}catch(Throwable $e){$thrown=true;}compat_check($thrown,'invalid general mixed inputs');}
$typed=Phit\fit_mixed(array_fill(0,8,[1]),[-3,-2,-1,1,2,3,4,5],
 [['z'=>array_fill(0,8,[1]),'groups'=>[1,1,'1','1','A','A','B','B']]], intercept: false);
compat_check($typed['random'][0]['group_labels']===[1,'1','A','B'],'general typed labels');
echo "PHP expanded mixed-model tests passed (15 models, four lme4 optimizers)\n";
