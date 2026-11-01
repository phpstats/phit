<?php
declare(strict_types=1);
$c=json_decode(stream_get_contents(STDIN),true,flags:JSON_THROW_ON_ERROR);
if(isset($c['model'])){$fn='Phit\fit_'.$c['model'].'_matrix';$r=$fn($c['x'],$c['y'],weights:$c['weights'],offset:$c['offset'], intercept: false);}
else {$r=Phit\fit_mixed_gaussian_matrix($c['x'],$c['y'],$c['terms'],reml:$c['reml'],weights:$c['weights'],offset:$c['offset'],optimizer:$c['optimizer']??'bobyqa', intercept: false);}
echo json_encode($r,JSON_THROW_ON_ERROR);
