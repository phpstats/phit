<?php
declare(strict_types=1);
$fixtures = json_decode(file_get_contents(__DIR__.'/../../tests/fixtures/iv-r.json'), true, flags: JSON_THROW_ON_ERROR);
set_error_handler(fn ($code,$message) => str_contains($message,'dropped'));
$output = [];
foreach ($fixtures['cases'] as $case) $output[$case['name']] = Phit\fit_iv($case['formula'],$case['data'],weights:$case['weights'])->toArray();
echo json_encode($output,JSON_THROW_ON_ERROR|JSON_PRESERVE_ZERO_FRACTION);
