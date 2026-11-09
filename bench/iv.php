<?php
declare(strict_types=1);
$fixture = json_decode(file_get_contents(__DIR__.'/../tests/fixtures/iv-r.json'), true, flags: JSON_THROW_ON_ERROR);
$name = $argv[1]; $copies = $name === 'heating_2160' ? 3 : 1;
$case = array_values(array_filter($fixture['cases'],fn ($c) => $c['name'] === ($copies === 3 ? 'heating' : $name)))[0];
$data = $case['data'];
if ($copies > 1) foreach ($data as &$column) $column = array_merge(...array_fill(0,$copies,$column));
unset($column);
$start = hrtime(true);
$fit = Phit\fit_iv($case['formula'],$data,weights:$case['weights']);
$elapsed = (hrtime(true)-$start)/1e6;
echo json_encode(['ms'=>$elapsed,'coefficients'=>$fit['coefficients'],'n'=>$fit['n_observations'],'k'=>count($fit['coefficients'])],JSON_THROW_ON_ERROR);
