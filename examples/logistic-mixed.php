<?php
declare(strict_types=1);
// Published cattle-disease data: new cases / animals at risk, clustered by herd.
// Source and license: docs/data/cbpp-source.json and docs/data/lmm/COPYING.
$handle=fopen(__DIR__.'/../docs/data/cbpp.csv','r');$header=fgetcsv($handle,escape:'');
$x=$y=$groups=$trials=[];
while(($row=fgetcsv($handle,escape:''))!==false){$r=array_combine($header,$row);$period=(int)$r['period'];
 $x[]=[(float)($period===2),(float)($period===3),(float)($period===4)];
 $trials[]=(int)$r['size'];$y[]=(int)$r['incidence']/(int)$r['size'];$groups[]=$r['herd'];}
fclose($handle);
foreach([1,9,25,61] as $points){
 $r=Phit\fit_logistic_mixed($x,$y,$groups,weights:$trials,quadraturePoints:$points);
 printf("\nBinomial random intercept: %s, %d nodes\n",$r['integration'],$points);
 foreach(['period 1','period 2 difference','period 3 difference','period 4 difference'] as $j=>$label)
  printf("%-22s beta %9.5f  SE %8.5f  p %9.4g\n",$label,$r['coefficients'][$j],$r['std_errors'][$j],$r['p_values'][$j]);
 printf("Random-intercept SD: %.6f; normalized log-likelihood: %.6f; AIC: %.6f\n",$r['random_intercept_sd'],$r['log_likelihood'],$r['aic']);
}
