<?php
// php -d extension=... bench/csv/memory.php FILE [phibble|read_csv]
// Reads FILE with phibble's DataFrame::fromCsv() (phit's reader when the extension is loaded) or with
// Phit\read_csv() directly, and prints the PHP heap the result holds and whether repeated strings are shared.
// Run it under /usr/bin/time -l for the peak memory footprint and resident set size.
declare(strict_types=1);

[, $path, $how] = $argv + [null, null, 'phibble'];
if ($how === 'phibble') {
    require getenv('PHIBBLE_AUTOLOAD') ?: throw new RuntimeException('Set PHIBBLE_AUTOLOAD to phibble\'s vendor/autoload.php');
}
$base = memory_get_usage();
if ($how === 'phibble') {
    $result = Phibble\DataFrame::fromCsv($path, types: ['placed_on' => 'c']);
    $region = $result->column('region');
} else {
    $result = Phit\read_csv($path, ['types' => ['i', 'c', 'c', 'c', 'c', 'i', 'd']]);
    $region = $result['columns'][2]['values'];
}
$heap = memory_get_usage() - $base;
ob_start();
debug_zval_dump($region[0]);
preg_match('/refcount\((\d+)\)/', (string) ob_get_clean(), $m);
printf("{\"heap_mib\": %.1f, \"region0_refcount\": %d}\n", $heap / 2**20, $m[1] ?? 0);
