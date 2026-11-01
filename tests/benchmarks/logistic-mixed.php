<?php
$start=hrtime(true);$file=fopen($argv[1],'r');fgetcsv($file,escape:'');$x=$y=$g=[];
while(($row=fgetcsv($file,escape:''))!==false){$v=array_map('floatval',$row);$y[]=array_shift($v);$g[]=(int)array_shift($v);$x[]=$v;}fclose($file);
$load=(hrtime(true)-$start)/1e9;$indices=[];for($j=0;$j<10;$j++)$indices[]=(int)floor($j*(count($y)-1)/9);
$r=Phit\fit_mixed_logistic_matrix($x,$y,$g, intercept: false);unset($r);$times=[];
for($i=0;$i<(int)$argv[2];$i++){$start=hrtime(true);$r=Phit\fit_mixed_logistic_matrix($x,$y,$g, intercept: false);$times[]=(hrtime(true)-$start)/1e9;
 $result=array_intersect_key($r,array_flip(['coefficients','covariance','std_errors','random_intercept_sd','random_effects','log_likelihood','converged']));
 foreach(['fitted','residuals','fixed_fitted','linear_predictor'] as $key)$result[$key.'_sample']=array_map(fn($j)=>$r[$key][$j],$indices);unset($r);}
echo json_encode(['version'=>PHP_VERSION,'load_seconds'=>$load,'fit_seconds'=>$times,'sample_indices'=>$indices,'result'=>$result],JSON_THROW_ON_ERROR);
