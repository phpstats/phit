<?php
declare(strict_types=1);
$fixture=json_decode(file_get_contents(__DIR__.'/../../tests/fixtures/vcov-r.json'),true,flags:JSON_THROW_ON_ERROR);
set_error_handler(fn($code,$message)=>str_contains($message,'dropped'));
$output=[];
foreach($fixture['cases'] as $case){
 $call='Phit\\'.$case['fun'];$fit=$call($case['formula'],$fixture['datasets'][$case['dataset']],weights:$case['weights'],vcov:$case['vcov']);
 $prediction=$fit->predict($fixture['datasets'][$case['dataset']],seFit:true,interval:'confidence');
 $actual=array_intersect_key($fit->toArray(),$case['expected']);
 foreach($case['prediction'] as $key=>$expected)$actual['prediction'][$key]=array_map(fn($i)=>$prediction[$key][$i],$fit['retained_rows']);
 $output[$case['name']]=$actual;
}
echo json_encode($output,JSON_THROW_ON_ERROR|JSON_PRESERVE_ZERO_FRACTION);
