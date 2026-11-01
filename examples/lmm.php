<?php
declare(strict_types=1);
// Synthetic students nested in schools, with a continuous assessment score.
$file=fopen(__DIR__.'/../docs/data/schools.csv','r');fgetcsv($file,escape:'');
$x=$y=$groups=[];
while(($row=fgetcsv($file,escape:''))!==false){$groups[]=$row[0];$x[]=[(float)$row[1],(float)$row[2]];$y[]=(float)$row[3];}
fclose($file);
$r=Phit\fit_mixed_gaussian_intercept_matrix(x:$x,y:$y,groups:$groups);
printf("%-16s %12s %12s %12s %12s\n",'Term','Estimate','Std. error','z','p-value');
foreach(['Intercept','Study hours','Prior score']as $j=>$label){printf("%-16s %12.6f %12.6f %12.6f %12.6g\n",$label,$r['coefficients'][$j],$r['std_errors'][$j],$r['z_statistics'][$j],$r['p_values'][$j]);}
printf("n=%d; groups=%d; ML residual variance=%.6f; group variance=%.6f; ICC=%.4f\n",$r['n_observations'],$r['n_groups'],$r['residual_variance'],$r['random_intercept_variance'],$r['icc']);
printf("Marginal log-likelihood=%.6f; AIC=%.6f; zero-group-variance boundary=%s\n",$r['log_likelihood'],$r['aic'],$r['boundary']?'yes':'no');
foreach($r['group_labels']as $j=>$label){printf("%s: random intercept % .4f (n=%d)\n",$label,$r['random_effects'][$j],$r['group_sizes'][$j]);}
printf("First student: population fit %.4f; fit with school effect %.4f\n",$r['fixed_fitted'][0],$r['fitted'][0]);
