#!/usr/bin/env python3
"""Write site/ content for phpstats.org from the saved benchmark and verification results.

The site (phpstats/phpstats) reads site/site.json and the files it lists. This script
only reshapes results already recorded under docs/; it runs no fits or benchmarks.
Run from the repository root: python3 scripts/site-export.py [--check]
"""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SITE = ROOT / 'site'
DOCS = ROOT / 'docs'

sys.path.insert(0, str(Path(__file__).resolve().parent))
import evidence  # noqa: E402


def version():
    import tomllib
    return tomllib.loads((ROOT / 'crates/php-extension/Cargo.toml').read_text())['package']['version']


def manifest(benchmarks, verifications):
    return {
        'package': 'phit',
        'title': 'phit',
        'tagline': 'Regression, mixed models, and probability distributions for PHP.',
        'summary': 'Fit statistical models, compute probability distributions, and summarize data in PHP. Model '
                   'functions accept formulas or explicit designs and return PHP arrays.',
        'install': ['pie install phpstats/phit'],
        'php': ['8.3', '8.4', '8.5'],
        'requires': ['PIE', 'unzip or the PHP zip extension', 'NTS (non-thread-safe) PHP'],
        'optional': [],
        'build': ['Rust toolchain', 'Clang and libclang', 'PHP development files (phpize and php-config)',
                  'autoconf and make'],
        'platforms': ['Linux', 'macOS'],
        'verify': {'command': "php -r 'echo phpversion(\"phit\"), PHP_EOL;'", 'output': version(),
                   'note': 'PHP-FPM reads its own php.ini, so enable the extension there and restart FPM.'},
        'installation': 'installation',
        'quickstart': 'getting-started',
        'repository': 'https://github.com/phpstats/phit',
        'example': {
            # The output is what this code prints with $housing read from docs/data/housing.csv.
            'chain': "use function Phit\\{fit_ols, predict};\n\n"
                     "// $housing: docs/data/housing.csv as named columns\n"
                     "$fit = fit_ols('monthly_rent ~ area_m2 + bedrooms', $housing);\n"
                     "foreach ($fit['named'] as $name => $b) {\n"
                     "    printf(\"%-12s %8.2f\\n\", $name, $b);\n"
                     "}\n\n"
                     "$p = predict(\n"
                     "    $fit,\n"
                     "    ['area_m2' => [60, 90], 'bedrooms' => [1, 3]],\n"
                     "    interval: 'prediction',\n"
                     ");\n"
                     "foreach ($p['fit'] as $i => $rent) {\n"
                     "    printf(\n"
                     "        \"%.2f [%.2f, %.2f]\\n\",\n"
                     "        $rent, $p['lower'][$i], $p['upper'][$i],\n"
                     "    );\n"
                     "}",
            'pipe': None,
            'output': "(Intercept)    516.18\n"
                      "area_m2         11.93\n"
                      "bedrooms       251.96\n"
                      "1484.12 [712.10, 2256.13]\n"
                      "2346.02 [1766.45, 2925.60]",
        },
        'docs': {
            'root': 'docs',
            'format': 'quarto',
            'sidebar': 'docs/_quarto.yml',
            # The site excludes data/ by default; phit's data/lmm page is in the nav.
            'exclude': [],
            'nav': [
                {'title': 'Overview', 'path': 'index.qmd'},
                {'title': 'Installation', 'path': 'installation.qmd'},
                {'title': 'Extension or polyfill', 'path': 'extension-or-polyfill.qmd'},
                {'title': 'A first model', 'path': 'getting-started.qmd'},
                {'title': 'Model formulas', 'path': 'formulas.qmd'},
                {'title': 'Linear and generalized linear models', 'path': 'models.qmd'},
                {'title': 'Mixed models', 'path': 'mixed-models.qmd'},
                {'title': 'Missing values and sample accounting', 'path': 'missing-data.qmd'},
                {'title': 'Probability distributions', 'path': 'distributions.qmd'},
                {'title': 'Descriptive statistics', 'path': 'descriptive.qmd'},
                {'title': 'Seeded sampling', 'path': 'rng.qmd'},
                {'title': 'Posterior draws', 'path': 'draw-numerics.qmd'},
                {'title': 'Reading CSV files', 'path': 'io.qmd'},
                {'title': 'Deploying to servers', 'path': 'deployment.qmd'},
                {'title': 'Fit a linear model and predict', 'path': 'examples/linear-model.qmd'},
                {'title': 'Fit logistic and Poisson models', 'path': 'examples/glm.qmd'},
                {'title': 'Fit a mixed model', 'path': 'examples/mixed-model.qmd'},
                {'title': 'Densities, quantiles, and random draws', 'path': 'examples/distributions.qmd'},
                {'title': 'Fit a mixed model to a Polars table and chart it', 'path': 'examples/polars-mixed-model.qmd'},
                {'title': 'API overview', 'path': 'contracts.qmd'},
                {'title': 'R compatibility', 'path': 'r-compatibility.qmd'},
                {'title': 'Development', 'path': 'development.qmd'},
            ],
        },
        'benchmarks': benchmarks,
        'evidence_gaps': {
            'benchmarks': 'No benchmark times formula parsing, predict(), descriptive statistics, kernel densities, '
                          'or chunked CsvReader reading.',
            'verification': 'The reference comparisons ran on macOS; Linux and PHP-FPM runs of them are not '
                            'recorded, though the release workflow installs and checks every Linux binary.',
        },
        'verifications': verifications,
        'examples': [
            {'path': 'examples/linear-model', 'title': 'Fit a linear model and predict',
             'description': 'Estimate rents from floor area and district and predict new flats with intervals.'},
            {'path': 'examples/glm', 'title': 'Fit logistic and Poisson models',
             'description': 'Read renewal odds ratios and monthly ticket rates, with an offset for observation time.'},
            {'path': 'examples/mixed-model', 'title': 'Fit a mixed model',
             'description': 'Give each school its own intercept and read how much schools vary.'},
            {'path': 'examples/distributions', 'title': 'Densities, quantiles, and random draws',
             'description': "Evaluate the normal and Student's t distributions and draw reproducible samples."},
            {'path': 'examples/polars-mixed-model', 'title': 'Fit a mixed model to a Polars table and chart it',
             'description': 'Read a CSV file with Polars, fit school intercepts, and chart one fitted line per school.',
             'also': ['phpolars', 'phiz']},
        ],
    }


def build():
    files = {}
    benchmarks, verifications = [], []
    for slug in evidence.MODEL_BENCH:
        files[f'benchmarks/{slug}.json'] = evidence.model_benchmark(slug)
    files['benchmarks/logistic-mixed.json'] = evidence.logistic_mixed_benchmark()
    files['benchmarks/distributions.json'] = evidence.distributions_benchmark()
    files['benchmarks/draws.json'] = evidence.draws_benchmark()
    files['benchmarks/rng.json'] = evidence.rng_benchmark()
    files['benchmarks/csv.json'] = evidence.csv_benchmark()
    benchmarks += [f'site/benchmarks/{slug}.json' for slug in
                   [*evidence.MODEL_BENCH, 'logistic-mixed', 'distributions', 'draws', 'rng', 'csv']]
    # Written by bench/run.py, not by this script.
    benchmarks += [f'site/benchmarks/{w}-workload.json'
                   for w in ['ols', 'mixed', 'logistic', 'probit', 'mixed-logistic', 'distributions']
                   if (SITE / 'benchmarks' / f'{w}-workload.json').is_file()]
    for slug in evidence.FIXED:
        files[f'verifications/{slug}.json'] = evidence.fixed_verification(slug)
    for slug in ['lmm', 'lmm-public']:
        files[f'verifications/{slug}.json'] = evidence.mixed_verification(slug)
    files['verifications/logistic-mixed.json'] = evidence.logistic_mixed_verification()
    files['verifications/r-compatibility.json'] = evidence.r_compatibility_verification()
    files['verifications/distributions.json'] = evidence.distributions_verification()
    files['verifications/test-suites.json'] = evidence.test_suites_verification()
    verifications += [f'site/verifications/{slug}.json' for slug in
                      [*evidence.FIXED, 'lmm', 'lmm-public', 'logistic-mixed', 'r-compatibility', 'distributions',
                       'test-suites']]
    files['site.json'] = manifest(benchmarks, verifications)
    return {path: json.dumps(content, indent=1, allow_nan=False) + '\n' for path, content in files.items()}


def main():
    files = build()
    if '--check' in sys.argv:
        stale = [p for p, text in files.items() if not (SITE / p).is_file() or (SITE / p).read_text() != text]
        if stale:
            raise SystemExit('site/ is out of date; run python3 scripts/site-export.py: ' + ', '.join(stale))
        print('Site content verified')
        return
    for path, text in files.items():
        (SITE / path).parent.mkdir(parents=True, exist_ok=True)
        (SITE / path).write_text(text)
    print(f'Wrote {len(files)} site files')


if __name__ == '__main__':
    main()
