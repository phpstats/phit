<?php
declare(strict_types=1);
$fixtures=json_decode(file_get_contents(__DIR__.'/../../tests/fixtures/ridge-r.json'),true,flags:JSON_THROW_ON_ERROR);
set_error_handler(fn($code,$message)=>str_contains($message,'dropped'));
$output=[];
foreach($fixtures['fits'] as $case) $output[$case['name']]=Phit\fit_ridge($case['formula'],$case['data'],$case['lambda'],penaltyFactor:$case['penaltyFactor'],standardize:$case['standardize'],bounds:$case['bounds'],weights:$case['weights'])->toArray();
foreach($fixtures['cv'] as $case) $output['cv/'.$case['cv_name']]=Phit\cv_ridge($case['formula'],$case['data'],lambda:$case['lambda'],foldid:$case['foldid'],penaltyFactor:$case['penaltyFactor'],standardize:$case['standardize'],bounds:$case['bounds'],weights:$case['weights']);
echo json_encode($output,JSON_THROW_ON_ERROR|JSON_PRESERVE_ZERO_FRACTION);
