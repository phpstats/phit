<?php

declare(strict_types=1);

/*
 * Runs the complete example scripts in docs/examples/scripts and writes what their pages include:
 *
 * - docs/examples/_generated/{name}.md: the script, its output, and its chart (the spec it actually produced)
 * - docs/examples/_phiz-scripts.html: the pinned vega, vega-lite, vega-embed and phiz loader script tags
 *
 * Each script runs in an empty directory beside a link to this checkout's vendor directory, with the data files it
 * reads copied from docs/data, as a reader would run it. phiz is a dev dependency for this script only. The
 * scripts also need the phit and phpolars extensions:
 *
 *     php -d extension=target/release/libphit.dylib -d extension=/path/to/libphp_polars.dylib \
 *         scripts/docs-examples.php
 */

use Phiz\Assets;
use Phiz\Chart;
use Phiz\Html;

require __DIR__.'/../vendor/autoload.php';

$root = dirname(__DIR__);
$docs = "{$root}/docs/examples";
@mkdir("{$docs}/_generated", 0777, true);
file_put_contents("{$docs}/_phiz-scripts.html", "<style>.phiz-figure{margin:0.5rem 0 1.5rem;overflow-x:auto}</style>\n".Assets::cdn()->scripts());

foreach (glob("{$docs}/scripts/*.php") as $file) {
    $name = basename($file, '.php');
    $code = trim((string) file_get_contents($file));
    $dir = sys_get_temp_dir().'/phit-example-'.bin2hex(random_bytes(6));
    mkdir($dir);
    symlink("{$root}/vendor", "{$dir}/vendor");
    preg_match_all("/'([\\w-]+\\.csv)'/", $code, $csv);
    foreach ($csv[1] as $data) {
        copy("{$root}/docs/data/{$data}", "{$dir}/{$data}");
    }
    $cwd = getcwd();
    chdir($dir);
    ob_start();
    $chart = (static function () use ($file) {
        require $file;

        return $chart;
    })();
    $output = trim((string) ob_get_clean());
    chdir($cwd);
    if (! $chart instanceof Chart) {
        fwrite(STDERR, "{$name}: expected \$chart to be a phiz Chart\n");
        exit(1);
    }
    preg_match('/^title: "(.+)"$/m', (string) @file_get_contents("{$docs}/{$name}.qmd"), $title);
    $figure = Html::container($chart, ['actions' => false], ['aria-label' => $title[1] ?? $name]);
    file_put_contents("{$docs}/_generated/{$name}.md", "## Code\n\n```php\n{$code}\n```\n\n## Output\n\n```\n{$output}\n```\n\n"
        ."```{=html}\n<div class=\"phiz-figure\">{$figure}</div>\n```\n");
    exec('rm -rf '.escapeshellarg($dir));
    echo "{$name}\n";
}
