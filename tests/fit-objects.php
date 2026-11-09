<?php

declare(strict_types=1);

// Fit and table objects: every model function returns the class its model key selects, reads like its array, is
// read-only, prints its summary, and agrees with the free functions; tidy() returns a Phit\Table.

function ocheck(bool $condition, string $message): void
{
    if (! $condition) {
        throw new RuntimeException($message);
    }
}

function throws(callable $call, string $class, ?string $message = null): bool
{
    try {
        $call();
    } catch (Throwable $e) {
        return $e instanceof $class && ($message === null || $e->getMessage() === $message);
    }

    return false;
}

$fixture = json_decode(file_get_contents(__DIR__.'/fixtures/summary/summary.json'), true, flags: JSON_THROW_ON_ERROR);
$classes = ['ridge'=>'Phit\RidgeFit','iv' => 'Phit\IvFit', 'linear' => 'Phit\LinearFit', 'generalized_linear' => 'Phit\GeneralizedLinearFit',
    'mixed_gaussian' => 'Phit\MixedGaussianFit', 'mixed_logistic' => 'Phit\MixedLogisticFit'];
$readOnly = 'A Phit fit is read-only; call toArray() for an array copy';

foreach ($fixture['cases'] as $case) {
    $args = $case['args'];
    if (isset($args['data'])) {
        $args['data'] = $fixture['data'][$args['data']];
    }
    $fit = ('Phit\\'.$case['function'])(...$args);
    $name = $case['name'];
    $array = $fit->toArray();

    ocheck(is_array($array) && ! is_array($fit), "$name: toArray()");
    ocheck(get_class($fit) === $classes[$array['model']], "$name: class ".get_class($fit));
    foreach (['Phit\Fit', ArrayAccess::class, Iterator::class, Countable::class, JsonSerializable::class, Stringable::class] as $interface) {
        ocheck($fit instanceof $interface, "$name: implements $interface");
    }
    ocheck((new ReflectionClass($fit))->isFinal(), "$name: final class");

    // Printing and the methods agree with the free functions, which also take the array.
    ocheck((string) $fit === $case['summary'] && $fit->summary() === $case['summary'], "$name: echo prints the summary");
    ocheck(Phit\summary($fit) === Phit\summary($array), "$name: summary() of the fit and of its array");
    ocheck((string) $fit->tidy(confInt: true) === (string) Phit\tidy($fit, confInt: true), "$name: \$fit->tidy() prints the table");
    ocheck(Phit\tidy($array, confInt: true)->toArray() === $fit->tidy(confInt: true)->toArray(), "$name: tidy() of the array");

    // Reading works as on the array: keys, nested values, isset, count, iteration, and JSON.
    foreach ($array as $key => $value) {
        ocheck($fit[$key] === $value && isset($fit[$key]) === isset($array[$key]), "$name: key $key");
    }
    ocheck(! isset($fit['no_such_key']) && ($fit['no_such_key'] ?? 'default') === 'default', "$name: missing key with ??");
    ocheck(count($fit) === count($array), "$name: count");
    ocheck(iterator_to_array($fit) === $array, "$name: iteration");
    $keys = [];
    foreach ($fit as $key => $value) {
        $keys[] = $key;
    }
    ocheck($keys === array_keys($array), "$name: foreach order");
    ocheck(json_encode($fit) === json_encode($array), "$name: JSON");
    ocheck($fit->toMimeBundle() === ['text/plain' => $case['summary'], 'application/json' => $array], "$name: toMimeBundle()");
    ocheck((clone $fit)->toArray() === $array, "$name: clone");

    // Writes are refused.
    ocheck(throws(function () use ($fit) {
        $fit['coefficients'] = [];
    }, LogicException::class, $readOnly), "$name: offsetSet throws");
    ocheck(throws(function () use ($fit) {
        unset($fit['coefficients']);
    }, LogicException::class, $readOnly), "$name: offsetUnset throws");

    // Formula fits predict through the method as through the function.
    if (! str_contains($case['function'], 'matrix')) {
        $newdata = $args['data'];
        ocheck($fit->predict($newdata) === Phit\predict($fit, $newdata), "$name: \$fit->predict()");
        ocheck(Phit\predict($array, $newdata) === Phit\predict($fit, $newdata), "$name: predict() of the array");
    }
}

// A missing key warns as on an array.
$fit = Phit\fit_ols('mpg ~ wt', $fixture['data']['mtcars']);
$warnings = [];
set_error_handler(function (int $level, string $message) use (&$warnings): bool {
    $warnings[] = [$level, $message];

    return true;
});
try {
    $missing = $fit['no_such_key'];
} finally {
    restore_error_handler();
}
ocheck($missing === null && $warnings === [[E_WARNING, 'Phit\LinearFit::offsetGet(): Undefined array key "no_such_key"']],
    'missing key warning: '.json_encode($warnings));
ocheck(throws(fn () => new Phit\LinearFit(), Throwable::class), 'fits cannot be constructed');
ocheck(throws(fn () => Phit\summary(new stdClass()), Exception::class,
    'fit must be a result of a Phit model function: not a fit object or array'), 'summary() of another object');

// Tables: read-only columns, row count, iteration over columns, and toColumns().
$table = $fit->tidy(confInt: true);
$columns = $table->toArray();
ocheck($table instanceof Phit\Table && (new ReflectionClass($table))->isFinal(), 'Table class');
ocheck($table->toColumns() === $columns && array_keys($columns) === ['term', 'estimate', 'std_error', 'statistic', 'p_value', 'conf_low', 'conf_high'], 'Table columns');
ocheck(count($table) === 2 && $table['term'] === ['(Intercept)', 'wt'] && iterator_to_array($table) === $columns, 'Table reads');
ocheck(json_encode($table) === json_encode($columns), 'Table JSON');
ocheck($table->toMimeBundle() === ['text/plain' => (string) $table, 'application/json' => $columns], 'Table toMimeBundle()');
ocheck(throws(function () use ($table) {
    $table['term'] = [];
}, LogicException::class, 'A Phit table is read-only; call toArray() for an array copy'), 'Table is read-only');

// phibble's DataFrame::fromColumns() reads a Table through toColumns() (phibble 0.8 or later).
$phibble = getenv('PHIBBLE_AUTOLOAD');
if ($phibble !== false && is_file($phibble)) {
    require $phibble;
    $frame = Phibble\DataFrame::fromColumns($table);
    ocheck($frame->toColumns()['estimate'] === $columns['estimate'], 'phibble reads a Table');
}

echo 'PHP fit and table objects passed ('.count($fixture['cases'])." fits, every model function)\n";
