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
        'tagline': 'Regression, instrumental variables, ridge, mixed models, and probability distributions for PHP.',
        'summary': 'Fit statistical models, compute probability distributions, and summarize data in PHP. Model '
                   'functions include OLS, instrumental variables, ridge with weighted cross-validation, GLMs, and mixed models. Fits print like R\'s and read like arrays.',
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
            # The output is what this code prints: tests/fixtures/summary/summary.json, case ols_mtcars.
            'chain': "use Phibble\\DataFrame;\n\n"
                     "use function Phit\\fit_ols;\n\n"
                     "$cars = DataFrame::fromCsv(\n"
                     "    'https://raw.githubusercontent.com/phpstats/datasets/main/csv/mtcars.csv',\n"
                     ");\n\n"
                     "$fit = fit_ols('mpg ~ wt + factor(cyl)', $cars);\n"
                     "echo $fit;",
            'pipe': None,
            'output': 'Linear model fit by least squares\nFormula: mpg ~ wt + factor(cyl)\n\nCoefficients:\n             Estimate Std. Error t value Pr(>|t|)\n(Intercept)   33.9908     1.8878  18.006  < 2e-16\nwt            -3.2056     0.7539  -4.252 0.000213\nfactor(cyl)6  -4.2556     1.3861  -3.070 0.004718\nfactor(cyl)8  -6.0709     1.6523  -3.674 0.000999\n\nResidual standard error: 2.557 on 28 degrees of freedom\nMultiple R-squared: 0.8374, Adjusted R-squared: 0.82\nF-statistic: 48.08 on 3 and 28 DF, p-value: 3.594e-11',
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
                {'title': 'Functions and methods', 'path': 'functions/index.qmd'},
                {'title': 'API conventions', 'path': 'contracts.qmd'},
                {'title': 'R compatibility', 'path': 'r-compatibility.qmd'},
                {'title': 'Development', 'path': 'development.qmd'},
            ],
        },
        'benchmarks': benchmarks,
        'evidence_gaps': {
            'benchmarks': 'No separately isolated benchmark times formula parsing, predict(), descriptive statistics, kernel densities, '
                          'or chunked CsvReader reading. IV/ridge/robust workload timings include formula construction.',
            'verification': 'The reference comparisons ran on macOS; Linux and PHP-FPM runs of them are not '
                            'recorded, though the release workflow installs and checks every Linux binary.',
        },
        'verifications': verifications,
        'examples': [
            {'path': 'examples/linear-model', 'title': 'Fit a linear model and predict',
             'description': 'Estimate fuel economy from weight and cylinders in mtcars and predict new cars with intervals.'},
            {'path': 'examples/glm', 'title': 'Fit logistic and Poisson models',
             'description': 'Model labor-force participation in mroz and insurance claim rates with an exposure offset.'},
            {'path': 'examples/mixed-model', 'title': 'Fit a mixed model',
             'description': 'Give each sleepstudy subject its own intercept and read how much subjects vary.'},
            {'path': 'examples/distributions', 'title': 'Densities, quantiles, and random draws',
             'description': "Evaluate the normal and Student's t distributions and draw reproducible samples."},
            {'path': 'examples/polars-mixed-model', 'title': 'Fit a mixed model to a Polars table and chart it',
             'description': 'Read sleepstudy with Polars, fit subject intercepts, and chart one fitted line per subject.',
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
        files[f'verifications/{slug}.json'] = evidence.readable(evidence.fixed_verification(slug), slug)
    for slug in ['lmm', 'lmm-public']:
        files[f'verifications/{slug}.json'] = evidence.readable(evidence.mixed_verification(slug), slug)
    files['verifications/logistic-mixed.json'] = evidence.readable(evidence.logistic_mixed_verification(), 'logistic-mixed')
    files['verifications/r-compatibility.json'] = evidence.readable(evidence.r_compatibility_verification(), 'r-compatibility')
    files['verifications/distributions.json'] = evidence.readable(evidence.distributions_verification(), 'distributions')
    files['verifications/test-suites.json'] = evidence.readable(evidence.test_suites_verification(), 'test-suites')
    verifications += [f'site/verifications/{slug}.json' for slug in
                      [*evidence.FIXED, 'lmm', 'lmm-public', 'logistic-mixed', 'r-compatibility', 'distributions',
                       'test-suites']]
    files['benchmarks/iv-workload.json'] = json.loads((ROOT / 'bench/iv-results.json').read_text())
    benchmarks.append('site/benchmarks/iv-workload.json')
    files['verifications/iv.json'] = evidence.iv_verification()
    verifications.append('site/verifications/iv.json')
    files['benchmarks/ridge-workload.json'] = json.loads((ROOT / 'bench/ridge-results.json').read_text())
    benchmarks.append('site/benchmarks/ridge-workload.json')
    files['verifications/ridge.json'] = evidence.ridge_verification()
    verifications.append('site/verifications/ridge.json')
    files['benchmarks/vcov-workload.json'] = json.loads((ROOT / 'bench/vcov-results.json').read_text())
    benchmarks.append('site/benchmarks/vcov-workload.json')
    files['verifications/vcov.json'] = evidence.vcov_verification()
    verifications.append('site/verifications/vcov.json')
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
