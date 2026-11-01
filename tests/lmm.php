<?php
declare(strict_types=1);
function mcheck(bool $condition, string $message): void {
    if (!$condition) throw new RuntimeException($message);
}
function mcompare(mixed $actual, mixed $expected, string $label, float $atol = 1e-7, float $rtol = 1e-6): void {
    if (is_array($expected)) {
        mcheck(count($actual) === count($expected), "$label shape");
        foreach ($expected as $i => $value) mcompare($actual[$i], $value, "$label/$i", $atol, $rtol);
    } else {
        mcheck(abs($actual - $expected) <= $atol + $rtol * abs($expected), "$label: $actual != $expected");
    }
}

$fixture = json_decode(file_get_contents(__DIR__.'/fixtures/lmm-r.json'),true,flags:JSON_THROW_ON_ERROR);
foreach ($fixture['cases'] as $case) {
 $r=Phit\fit_mixed_gaussian_intercept_matrix($case['x'],$case['y'],$case['groups'], intercept: false);
 foreach($case['reference']['result'] as $key=>$expected) mcompare($r[$key],$expected,$case['name'].'/'.$key);
 mcheck($r['method']==='ML' && $r['converged'] && $r['evaluations']>=66,'fit metadata');
 mcheck($r['group_labels']===array_values(array_unique($case['groups'])),'group order');
}
$published = json_decode(file_get_contents(__DIR__.'/fixtures/lmm-public-r.json'),true,flags:JSON_THROW_ON_ERROR);
foreach ($published['cases'] as $case) {
 $groups = array_map(fn($g) => $case['group_labels'][$g], $case['groups']);
 $r = Phit\fit_mixed_gaussian_intercept_matrix($case['x'], $case['y'], $groups, intercept: false);
 foreach (['reference', 'nlme_reference'] as $backend) {
  foreach ($case[$backend]['result'] as $key => $expected) {
   $atol = $case['tolerance']['quantity_absolute'][$key] ?? $case['tolerance']['absolute'];
   $rtol = $case['tolerance']['quantity_relative'][$key] ?? $case['tolerance']['relative'];
   mcompare($r[$key], $expected, $case['name'].'/'.$backend.'/'.$key, $atol, $rtol);
  }
 }
 mcheck($r['group_labels'] === $case['group_labels'], 'published group label order');
 mcheck($r['n_observations'] === count($case['y']) && $r['n_dropped'] === 0, 'published sample accounting');
}
foreach([
 [[],[],[]], [[[1],[1],[1]],[1,2,3],[1,2]],
 [[[1],[1],[1]],[1,2,3],[1,1,1]], [[[1],[1],[1]],[1,2,3],[1,2,3]],
 [[[1],[1],[1],[1]],[1,2,3,4],[1,1,true,2]],
 [[[null],[1],[1],[1]],[1,2,3,4],[1.5,1,2,2]],
 [[[1],[1],[1],[1]],[1,2,3,4],[1,1,2.0,2]],
 [[[1],[1],[1],[1]],[2,2,2,2],[1,1,2,2]],
 [[[1],[1],[1],[1]],[1,1,2,2],[1,1,2,2]],
] as [$x,$y,$g]) { $thrown=false;try{Phit\fit_mixed_gaussian_intercept_matrix($x,$y,$g, intercept: false);}catch(Throwable $e){$thrown=true;}mcheck($thrown,'invalid/group boundary must fail');}
$c=$fixture['cases'][0];$x=$c['x'];$y=$c['y'];$g=$c['groups'];
$x[1][1]=null;$y[4]=NAN;$g[8]=null;
$warnings=[];set_error_handler(function($severity,$message)use(&$warnings){$warnings[]=[$severity,$message];return true;});
try{$r=Phit\fit_mixed_gaussian_intercept_matrix($x,$y,$g, intercept: false);}finally{restore_error_handler();}
mcheck(count($warnings)===1 && $warnings[0][0]===E_WARNING && str_contains($warnings[0][1],'Phit\fit_mixed_gaussian_intercept_matrix'),'missing warning');
mcheck($r['dropped_rows']===[1,4,8] && $r['n_dropped']===3,'all three arrays use listwise deletion');
$xx=$yy=$gg=[];foreach($x as $i=>$row)if(!in_array($i,[1,4,8],true)){$xx[]=$row;$yy[]=$y[$i];$gg[]=$g[$i];}
$complete=Phit\fit_mixed_gaussian_intercept_matrix($xx,$yy,$gg, intercept: false);mcompare($r['fitted'],$complete['fitted'],'retained ordering');
foreach(['raise','unknown']as $policy){$thrown=false;try{Phit\fit_mixed_gaussian_intercept_matrix($x,$y,$g,$policy, intercept: false);}catch(Throwable $e){$thrown=true;}mcheck($thrown,'missing policy');}
// Integer and string labels are distinct even when their printed text matches.
$c=$fixture['cases'][5];$ids=[1,1,1,'1','1','1','A','A','A','B','B','B'];
$r=Phit\fit_mixed_gaussian_intercept_matrix($c['x'],$c['y'],$ids, intercept: false);mcheck($r['group_labels']===[1,'1','A','B'],'typed label identities');
echo "PHP mixed-model integration tests passed\n";
