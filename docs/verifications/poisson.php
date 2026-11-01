<?php
$input = json_decode(stream_get_contents(STDIN),true,flags:JSON_THROW_ON_ERROR);
$warnings = [];
set_error_handler(function (int $severity,string $message) use (&$warnings): bool {
    $warnings[] = ['severity'=>$severity,'message'=>$message]; return true;
});
try { $result = Phit\fit_poisson_matrix(x:$input['x'],y:$input['y'], intercept: false); }
finally { restore_error_handler(); }
echo json_encode(['version'=>PHP_VERSION,'result'=>$result,'warnings'=>$warnings],JSON_THROW_ON_ERROR);
