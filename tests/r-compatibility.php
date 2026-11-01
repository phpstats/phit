<?php
declare(strict_types=1);
function compat_check(bool $ok,string $label):void {if(!$ok)throw new RuntimeException($label);}
function compat_compare(mixed $a,mixed $b,string $label,float $abs=1e-7,float $rel=1e-6):void {
 if(is_array($b)){compat_check(count($a)===count($b),"$label shape");foreach($b as $k=>$v)compat_compare($a[$k],$v,"$label/$k",$abs,$rel);}
 elseif(is_bool($b)){compat_check($a===$b,"$label bool");}
 else {compat_check(is_finite((float)$a)&&abs($a-$b)<=$abs+$rel*abs($b),"$label: $a != $b");}
}
$f=json_decode(file_get_contents(__DIR__.'/fixtures/r-compatibility.json'),true,flags:JSON_THROW_ON_ERROR);
foreach($f['cases'] as $c){$fn='Phit\fit_'.$c['model'].'_matrix';$r=$fn(x:$c['x'],y:$c['y'],weights:$c['weights'],offset:$c['offset'], intercept: false);
 foreach($c['expected'] as $k=>$v)compat_compare($r[$k],$v,$c['name'].'/'.$k);}
// Missing optional values participate in the same deletion mask as x and y.
foreach(['ols','logistic','probit','poisson'] as $model){
 $c=array_values(array_filter($f['cases'],fn($c)=>$c['model']===$model))[0];$c['weights'][3]=null;$c['offset'][5]=NAN;
 $warnings=[];set_error_handler(function($severity,$message)use(&$warnings){$warnings[]=[$severity,$message];return true;});
 try{$fn='Phit\fit_'.$model.'_matrix';$r=$fn($c['x'],$c['y'],weights:$c['weights'],offset:$c['offset'], intercept: false);}finally{restore_error_handler();}
 compat_check($r['dropped_rows']===[3,5]&&count($warnings)===1,'weighted missing rows');
 $select=fn($v)=>array_values(array_filter($v,fn($i)=>!in_array($i,[3,5],true),ARRAY_FILTER_USE_KEY));
 $complete=$fn($select($c['x']),$select($c['y']),weights:$select($c['weights']),offset:$select($c['offset']), intercept: false);
 compat_compare($r['coefficients'],$complete['coefficients'],'weighted deletion alignment');
 foreach([['weights'=>[0]],['weights'=>array_fill(0,count($c['y']),-1)],['offset'=>[0]],['weights'=>['1']],['missing'=>'raise']] as $options){
  $thrown=false;try{$fn(...array_replace(['x'=>$c['x'],'y'=>$c['y'],'weights'=>$c['weights'],'offset'=>$c['offset']],$options), intercept: false);}catch(Throwable $e){$thrown=true;}compat_check($thrown,'invalid weighted inputs');
 }
}
echo "PHP base-R compatibility tests passed\n";
