<?php
declare(strict_types=1);
$fixture=json_decode(file_get_contents(__DIR__.'/../tests/fixtures/vcov-r.json'),true,flags:JSON_THROW_ON_ERROR);
$case=array_values(array_filter($fixture['cases'],fn($c)=>$c['name']===$argv[1]))[0];
$data=$fixture['datasets'][$case['dataset']];$call='Phit\\'.$case['fun'];
$start=hrtime(true);$fit=$call($case['formula'],$data,weights:$case['weights'],vcov:$case['vcov']);$elapsed=(hrtime(true)-$start)/1e6;
echo json_encode(['ms'=>$elapsed,'coefficients'=>array_merge($fit['coefficients'],...$fit['covariance']),'n'=>$fit['n_observations'],'k'=>count($fit['coefficients'])],JSON_THROW_ON_ERROR);
