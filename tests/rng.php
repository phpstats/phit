<?php
declare(strict_types=1);
function check(bool $ok, string $message): void {
    if (!$ok) throw new RuntimeException($message);
}
function code(callable $f, int $expected): void {
    try { $f(); } catch (Throwable $e) {
        check($e->getCode() === $expected, "{$e->getMessage()}: code {$e->getCode()} != $expected");
        return;
    }
    throw new RuntimeException("Expected error $expected");
}
$fixture = json_decode(file_get_contents(__DIR__.'/fixtures/rng-python-r.json'), true, flags: JSON_THROW_ON_ERROR);
$compared = 0;
foreach ($fixture['cases'] as $c) {
    if ($c['distribution'] === 'uniform') continue; // Core-only engine reference.
    $fn = 'Phit\dist_'.$c['distribution'].'_rng';
    $draws = $fn(...['n'=>$c['n'], 'seed'=>$c['seed'], ...$c['params']]);
    check(array_is_list($draws) && count($draws)===$c['n'], "$fn packed shape");
    check($draws === $fn($c['n'], $c['seed'], ...$c['params']), "$fn repeatability");
    check(array_slice($draws,0,7) === $fn(7,$c['seed'],...$c['params']), "$fn prefix stability");
    check($fn(0,$c['seed'],...$c['params']) === [], "$fn empty batch");
    foreach ($draws as $i => $v) {
        $expected = $c['expected'][$i];
        check(is_float($v) && is_finite($v), "$fn numeric output");
        if (in_array($c['distribution'], ['binomial','poisson','nbinom','nbinom_mu'], true)) {
            check($v == $expected && $v >= 0 && floor($v)===$v, "$fn count reference");
        } else {
            check(abs($v-$expected) <= 1e-15+1e-12*abs($expected), "$fn: $v != $expected");
        }
        $compared++;
    }
    code(fn() => $fn(-1,$c['seed'],...$c['params']), 1001);
    code(fn() => $fn(1,-1,...$c['params']), 1001);
    code(fn() => $fn(1000001,$c['seed'],...$c['params']), 1007);
}
// Every call has local state; sampling another family cannot advance this stream.
$first = Phit\dist_normal_rng(128,42);
Phit\dist_gamma_rng(128,99,shape: 3);
check($first === Phit\dist_normal_rng(128,42), 'no shared stream');
check($first !== Phit\dist_normal_rng(128,43), 'different seeds');
check($first === Phit\dist_normal_rng(128,42,mean:0,sd:1), 'normal defaults and integer parameters');
check(Phit\dist_lognormal_rng(5,42) === Phit\dist_lognormal_rng(5,42,meanlog:0,sdlog:1), 'lognormal defaults');
check(Phit\dist_gamma_rng(5,42,shape:2) === Phit\dist_gamma_rng(5,42,shape:2,scale:1), 'gamma default');
check(Phit\dist_exponential_rng(5,42) === Phit\dist_exponential_rng(5,42,rate:1), 'exponential default');
check(Phit\dist_logistic_rng(5,42) === Phit\dist_logistic_rng(5,42,location:0,scale:1), 'logistic defaults');
// Existing family validation applies even to an empty requested batch.
foreach ([fn() => Phit\dist_normal_rng(0,42,sd:0), fn() => Phit\dist_t_rng(0,42,df:0),
    fn() => Phit\dist_beta_rng(0,42,alpha:-1,beta:2), fn() => Phit\dist_gamma_rng(0,42,shape:0),
    fn() => Phit\dist_binomial_rng(0,42,size:-1,prob:.5), fn() => Phit\dist_poisson_rng(0,42,lambda:-1),
    fn() => Phit\dist_nbinom_rng(0,42,size:2,prob:0), fn() => Phit\dist_nbinom_mu_rng(0,42,size:0,mu:3)] as $f) code($f,1001);
foreach ([NAN,INF,-INF] as $v) {
    code(fn() => Phit\dist_normal_rng(0,42,mean:$v),1003);
    code(fn() => Phit\dist_poisson_rng(0,42,lambda:$v),1003);
}
code(fn() => Phit\dist_binomial_rng(0,42,size:2**54,prob:.5),1007);
code(fn() => Phit\dist_normal_rng(1,42,mean:PHP_FLOAT_MAX,sd:PHP_FLOAT_MAX),1003);
foreach ([null,'42',true,1.5,[],NAN,INF] as $bad) {
    // ext-php-rs argument conversion may throw Exception rather than Zend's
    // TypeError; both are runtime errors with code zero, outside StatsError.
    foreach ([fn() => Phit\dist_normal_rng(1,$bad), fn() => Phit\dist_normal_rng($bad,42)] as $f) {
        $rejected = false;
        try { $f(); } catch (Throwable $e) { $rejected = true; check($e->getCode()===0,'runtime argument category'); }
        check($rejected,'integer seed/count type rejected');
    }
}
// Deterministic shape/moment checks complement the exact reference vectors.
$normal = Phit\dist_normal_rng(50000,173,mean:3,sd:2);
$poisson = Phit\dist_poisson_rng(50000,174,lambda:12);
check(abs(Phit\stat_mean($normal)-3)<.06 && abs(Phit\stat_var($normal,'population')-4)<.15, 'normal moments');
check(abs(Phit\stat_mean($poisson)-12)<.1 && abs(Phit\stat_var($poisson,'population')-12)<.4, 'Poisson moments');
// Uniform, categorical and permutation samplers consume the same uniforms;
// the references below apply the documented maps to the Python uniforms.
$uniform = [];
foreach ($fixture['cases'] as $c) if ($c['distribution'] === 'uniform') $uniform[$c['seed']] = $c['expected'];
foreach ($uniform as $seed => $u) {
    check(Phit\dist_uniform_rng(96, $seed) === $u, "runif seed $seed");
    check(Phit\dist_uniform_rng(96, $seed, min: -2, max: 3) === array_map(fn($u) => -2 + 5 * $u, $u), "runif(-2, 3) seed $seed");
    $prob = [.2, 0, .5, .3];
    $expected = array_map(fn($u) => $u < .2 ? 0 : ($u < .7 ? 2 : 3), $u);
    $draws = Phit\dist_categorical_rng(96, $seed, $prob);
    check($draws === $expected, "categorical seed $seed");
    check(Phit\dist_categorical_rng(96, $seed, [2, 0, 5, 3]) === $draws, 'weights are normalized');
    check(Phit\dist_categorical_rows_rng($seed, array_fill(0, 96, $prob)) === $draws, 'equal rows reproduce one vector');
    $order = range(0, 49);
    for ($i = 0; $i < 50; $i++) {
        $j = $i + min((int) ($u[$i] * (50 - $i)), 49 - $i);
        [$order[$i], $order[$j]] = [$order[$j], $order[$i]];
    }
    check(Phit\permutation_rng(50, $seed) === $order, "permutation seed $seed");
    check(Phit\permutation_rng(50, $seed, size: 7) === array_slice($order, 0, 7), 'partial permutation prefix');
    $compared += 5 * 96 + 57;
}
check(Phit\dist_uniform_rng(3, 1, min: 4, max: 4) === [4.0, 4.0, 4.0], 'degenerate uniform');
check(Phit\dist_categorical_rows_rng(9, [[1, 0], [0, 1], [0, 0, 1]]) === [0, 1, 2], 'per-row probabilities');
check(Phit\dist_categorical_rows_rng(9, []) === [] && Phit\permutation_rng(0, 9) === [], 'empty samplers');
check(Phit\dist_categorical_rng(1000, 7, [0, 1, 0]) === array_fill(0, 1000, 1), 'zero weights never drawn');
$sorted = Phit\permutation_rng(1000, 5); sort($sorted);
check($sorted === range(0, 999), 'permutation of all indices');
$counts = array_count_values(Phit\dist_categorical_rng(50000, 175, [.1, .6, .3]));
check(abs($counts[0] / 50000 - .1) < .01 && abs($counts[1] / 50000 - .6) < .01, 'categorical frequencies');
foreach ([fn() => Phit\dist_uniform_rng(0, 1, min: 1, max: 0), fn() => Phit\dist_categorical_rng(0, 1, []),
    fn() => Phit\dist_categorical_rng(0, 1, [-1, 2]), fn() => Phit\dist_categorical_rng(0, 1, [0, 0]),
    fn() => Phit\dist_categorical_rng(0, 1, ['a' => 1]), fn() => Phit\dist_categorical_rng(0, 1, ['1']),
    fn() => Phit\dist_categorical_rows_rng(1, [[1], 1]), fn() => Phit\dist_categorical_rows_rng(1, [[1], []]),
    fn() => Phit\dist_categorical_rows_rng(-1, [[1]]), fn() => Phit\permutation_rng(3, 1, size: 4),
    fn() => Phit\permutation_rng(3, 1, size: -1), fn() => Phit\permutation_rng(-1, 1)] as $f) code($f, 1001);
foreach ([fn() => Phit\dist_uniform_rng(0, 1, min: NAN), fn() => Phit\dist_uniform_rng(1, 1, min: -PHP_FLOAT_MAX, max: PHP_FLOAT_MAX),
    fn() => Phit\dist_categorical_rng(0, 1, [INF]), fn() => Phit\dist_categorical_rng(0, 1, [PHP_FLOAT_MAX, PHP_FLOAT_MAX])] as $f) code($f, 1003);
foreach ([fn() => Phit\dist_uniform_rng(1000001, 1), fn() => Phit\dist_categorical_rng(1000001, 1, [1]),
    fn() => Phit\permutation_rng(1000001, 1, size: 0)] as $f) code($f, 1007);
echo "PHP RNG tests passed ($compared independent R/ChaCha values, seed isolation, validation and moments; uniform, categorical and permutation samplers)\n";
