<?php
declare(strict_types=1);
// Published sleepstudy: reaction time (ms) versus days of sleep deprivation.
// Data provenance/licensing: docs/data/lmm/index.qmd and COPYING.
$handle=fopen(__DIR__.'/../docs/data/lmm/sleepstudy.csv','r');
$header=fgetcsv($handle,escape:'');$x=$y=$groups=[];
while(($row=fgetcsv($handle,escape:''))!==false){$observation=array_combine($header,$row);
 $x[]=[(float)$observation['Days']];$y[]=(float)$observation['Reaction'];$groups[]=$observation['Subject'];}
fclose($handle);
$result=Phit\fit_mixed_gaussian_matrix(x:$x,y:$y,random:[['z'=>array_map(fn($row)=>[1.0, ...$row],$x),'groups'=>$groups]],reml:true,optimizer:'bobyqa');
printf("sleepstudy: Reaction ~ Days + (Days | Subject), %s\n",$result['method']);
foreach(['intercept','Days'] as $j=>$name)printf("%-10s coefficient %10.6f  SE %9.6f  Wald z %9.4f\n",$name,$result['coefficients'][$j],$result['std_errors'][$j],$result['z_statistics'][$j]);
printf("Residual variance: %.6f\nRestricted log-likelihood: %.6f\n",$result['residual_variance'],$result['log_likelihood']);
echo "Random intercept/slope covariance:\n";
foreach($result['random'][0]['covariance'] as $row)printf("[%12.6f %12.6f]\n",...$row);
printf("Optimizer: %s (%s); singular: %s\n",$result['optimizer'],$result['optimizer_implementation'],$result['singular']?'yes':'no');
