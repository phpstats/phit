<?php
declare(strict_types=1);
$fixture=json_decode(file_get_contents(__DIR__.'/../tests/fixtures/ridge-r.json'),true,flags:JSON_THROW_ON_ERROR);
$name=$argv[1];$isCv=$name==='cv_heating';$copies=$name==='heating_1152'?4:1;
$case=array_values(array_filter($fixture[$isCv?'cv':'fits'],fn($c)=>($c[$isCv?'cv_name':'name'])===($isCv?'day_folds_with_age_weights':($name==='standardized'?'standardized':'heating'))))[0];
$data=$case['data'];
if($copies>1) foreach($data as &$column) $column=array_merge(...array_fill(0,$copies,$column));
unset($column);
$start=hrtime(true);
if($isCv) {
 $result=Phit\cv_ridge($case['formula'],$data,lambda:$case['lambda'],foldid:$case['foldid'],penaltyFactor:$case['penaltyFactor'],standardize:$case['standardize'],bounds:$case['bounds'],weights:$case['weights']);
 $fit=$result['fit'];$check=array_merge($fit['coefficients'],$result['cvm'],$result['cvsd'],[$result['lambda_min'],$result['lambda_1se']]);
} else {
 $fit=Phit\fit_ridge($case['formula'],$data,$case['lambda'],penaltyFactor:$case['penaltyFactor'],standardize:$case['standardize'],bounds:$case['bounds'],weights:$case['weights']);
 $check=$fit['coefficients'];
}
$elapsed=(hrtime(true)-$start)/1e6;
echo json_encode(['ms'=>$elapsed,'coefficients'=>$check,'n'=>$fit['n_observations'],'k'=>count($fit['coefficients'])],JSON_THROW_ON_ERROR);
