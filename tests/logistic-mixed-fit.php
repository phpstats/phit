<?php
declare(strict_types=1);
$c=json_decode(stream_get_contents(STDIN),true,flags:JSON_THROW_ON_ERROR);
$r=Phit\fit_logistic_mixed($c['x'],$c['y'],$c['groups'],weights:$c['weights'],offset:$c['offset'],quadraturePoints:$c['quadrature_points'], intercept: false);
echo json_encode($r,JSON_THROW_ON_ERROR);
