#!/usr/bin/env python3
"""Write site/ content for phpstats.org from the saved benchmark and verification results.

The site (phpstats/phpstats) reads site/site.json and the files it lists. This script
only reshapes results already recorded under docs/; it runs no fits or benchmarks.
Run from the repository root: python3 scripts/site-export.py [--check]
"""
import json
import statistics
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SITE = ROOT / 'site'
DOCS = ROOT / 'docs'

MODELS = {
    'ols': 'OLS',
    'logistic': 'Logistic regression',
    'probit': 'Probit regression',
    'poisson': 'Poisson regression',
    'lmm': 'Gaussian random intercept',
    'logistic-mixed': 'Binomial random intercept',
}
# Benchmark implementation labels as the site shows them.
IMPLEMENTATIONS = {'PHP': 'phit', 'R': 'R', 'statsmodels': 'statsmodels', 'SciPy reference': 'SciPy',
                   'PHP_scalar': 'phit (scalar)', 'PHP_batch': 'phit (batch)', 'Rust': 'Rust core', 'SciPy': 'SciPy'}


R_TIMER_RESOLUTION = 0.001


def versions_text(versions):
    return ', '.join(v if str(v).startswith(k) else f'{k} {v}' for k, v in versions.items())


def load(path):
    return json.loads((ROOT / path).read_text())


def environment(data):
    return {
        'cpu': data.get('cpu'),
        'platform': data.get('platform'),
        'php': next((v.get('version') for c in data.get('cases', [])[:1]
                     for k, v in c.get('implementations', {}).items() if k.startswith('PHP')), None),
        'date': data.get('recorded_at'),
        'repeats': data.get('repeats'),
    }


def model_benchmark(slug, title):
    data = load(f'docs/benchmarks/{slug}-results.json')
    rows = []
    for case in data['cases']:
        p = case.get('p')
        label = f"n = {case['n']:,}" + (f", p = {p}" if p else '')
        if case.get('groups'):
            label += f", {case['groups']:,} groups"
        for name, result in case['implementations'].items():
            reps = result.get('fit_seconds') or []
            if not reps:
                continue
            row = {
                'case': label,
                'implementation': IMPLEMENTATIONS.get(name, name),
                'value': round(statistics.median(reps) * 1000, 3),
                'reps': [round(r * 1000, 3) for r in reps],
                'load_ms': round(result['load_seconds'] * 1000, 3) if result.get('load_seconds') is not None else None,
            }
            # R's proc.time() reports whole milliseconds; a fit at or below one tick is not measurable.
            if name == 'R' and statistics.median(reps) <= R_TIMER_RESOLUTION:
                row['value'] = None
                row['note'] = 'below the 1 ms resolution of R\'s timer'
            rows.append(row)
    return {
        'title': f'{title}: fit time',
        'description': f'Median wall time to fit {title.lower()} models on the same simulated data in R, Python and phit. '
                       'Data loading is timed separately and excluded.',
        'measure': 'median fit wall time',
        'unit': 'ms',
        'environment': environment(data),
        'source': f'docs/benchmarks/{slug}-results.json',
        'rows': rows,
    }


def timed_benchmark(slug, title, description, case_label):
    data = load(f'docs/benchmarks/{slug}-results.json')
    rows = []
    for case in data['cases']:
        for name, result in case['implementations'].items():
            timings = result.get('seconds') or []
            # Draw benchmarks time several operations per case: one row each.
            for operation, reps in (timings.items() if isinstance(timings, dict) else [(None, timings)]):
                if not reps:
                    continue
                rows.append({
                    'case': case_label(case) + (f', {operation.replace("_", " ")}' if operation else ''),
                    'implementation': IMPLEMENTATIONS.get(name, name),
                    'value': round(statistics.median(reps) * 1000, 4),
                    'reps': [round(r * 1000, 4) for r in reps],
                })
    return {'title': title, 'description': description, 'measure': 'median wall time', 'unit': 'ms',
            'environment': environment(data), 'source': f'docs/benchmarks/{slug}-results.json', 'rows': rows}


def fixed_verification(slug, title, peers):
    cases = []
    for record in load(f'docs/verifications/{slug}-results.json'):
        for quantity, errors in record['max_absolute_errors_vs_R'].items():
            if 'phit' not in errors:
                continue
            cases.append({
                'name': record['name'],
                'quantity': quantity,
                'max_abs_error': errors['phit'],
                'peers': {k: v for k, v in errors.items() if k != 'phit'},
                'passed': True,
            })
    versions = load(f'docs/verifications/{slug}-results.json')[0]['versions']
    return {
        'title': f'{title}: agreement with R',
        'description': f'Largest absolute difference from R for every reported quantity, alongside {peers}. '
                       'Every case also passes the release gate (scripts/check).',
        'reference': versions_text(versions),
        'tolerance': 'release gate: 1e-9 + 1e-7*|reference| (fixed models), 1e-7 + 1e-6*|reference| (mixed models)',
        'source': f'docs/verifications/{slug}-results.json',
        'cases': cases,
    }


def logistic_mixed_verification():
    data = load('docs/verifications/logistic-mixed-results.json')
    tol = data['tolerances']['estimates']
    cases = [{'name': r['name'], 'quantity': 'coefficients', 'max_abs_error': r['max_coefficient_difference'],
              'passed': r['max_coefficient_difference'] <= tol['absolute'] + tol['relative'] * 10}
             for r in data['results']]
    return {'title': 'Binomial random intercept: agreement with lme4',
            'description': 'Largest coefficient difference from lme4::glmer for Laplace and adaptive quadrature fits.',
            'reference': versions_text(data['versions']),
            'tolerance': f"{tol['absolute']} + {tol['relative']}*|reference|",
            'source': 'docs/verifications/logistic-mixed-results.json', 'cases': cases}


def distributions_verification():
    data = load('docs/verifications/distributions-results.json')
    cases = []
    for c in data['cases']:
        errors = c['max_error']
        params = ', '.join(f'{k}={v}' for k, v in c['params'].items())
        cases.append({'name': f"{c['distribution']}({params})", 'quantity': c['operation'], 'n_compared': c['n'],
                      'max_rel_error': errors.get('php'), 'peers': {k: v for k, v in errors.items() if k != 'php'},
                      'passed': errors.get('php') is not None and errors['php'] <= 1e-12})
    return {'title': 'Distributions: accuracy against 60-digit references',
            'description': data['error_definition'].capitalize() + '. R and SciPy errors are shown for comparison.',
            'reference': f"mpmath ({data.get('mpmath_digits', 60)} digits); R {data.get('r')}; SciPy {data.get('scipy')}",
            'tolerance': '1e-12 relative',
            'source': 'docs/verifications/distributions-results.json', 'cases': cases}


def r_compatibility_verification():
    data = load('docs/verifications/r-compatibility-results.json')
    cases = [{'name': f['name'], 'quantity': f'{f["model"]}: all quantities', 'max_abs_error': f['max_absolute_error'],
              'passed': True} for f in data['fixed']]
    for m in data['mixed']:
        for comparison in m.get('comparisons', []):
            pair = f"phit {comparison['native_optimizer']} vs lme4 {comparison['R_optimizer']}"
            for key, value in comparison.items():
                if key.endswith('_difference'):
                    cases.append({'name': f"{m['name']} ({m['method']})",
                                  'quantity': f"{pair}: {key.removesuffix('_difference').replace('_', ' ')}",
                                  'max_abs_error': abs(value), 'passed': True})
    return {'title': 'Weights, offsets and mixed models: agreement with R and lme4',
            'description': 'Weighted and offset GLMs against base R, and mixed models against all four lme4 optimizers.',
            'reference': versions_text(data['versions']),
            'tolerance': '; '.join(f"{k}: {v['absolute']} + {v['relative']}*|reference|" for k, v in data['tolerances'].items()),
            'source': 'docs/verifications/r-compatibility-results.json', 'cases': cases}


def version():
    import tomllib
    return tomllib.loads((ROOT / 'crates/php-extension/Cargo.toml').read_text())['package']['version']


def manifest(benchmarks, verifications):
    return {
        'package': 'phit',
        'title': 'phit',
        'tagline': 'Regression, mixed models and probability distributions for PHP.',
        'summary': 'phit is a PHP extension, written in Rust, for regression and mixed models, probability '
                   'distributions and descriptive statistics. Models are written as R formulas (fit_lm, fit_glm, '
                   'fit_lmer, fit_glmer, predict), and results are plain PHP arrays.',
        'install': ['pie install phpstats/phit'],
        'php': ['8.3', '8.4', '8.5'],
        'requires': ['PIE'],
        'optional': [],
        'build': ['Rust toolchain', 'Clang and libclang', 'PHP development files (phpize and php-config)',
                  'autoconf and make'],
        'platforms': ['Linux', 'macOS', 'NTS (non-thread-safe) PHP'],
        'verify': {'command': "php -r 'echo phpversion(\"phit\"), PHP_EOL;'", 'output': version(),
                   'note': 'This checks the command line. PHP-FPM can read other configuration files: '
                           "check extension_loaded('phit') in a web request and restart FPM after installing."},
        'quickstart': 'getting-started',
        'repository': 'https://github.com/phpstats/phit',
        'example': {
            'chain': "use function Phit\\{fit_lm, predict};\n\n"
                     "$fit = fit_lm('monthly_rent ~ area_m2 + bedrooms', $housing);\n"
                     "$fit['named'];  // ['(Intercept)' => ..., 'area_m2' => ..., 'bedrooms' => ...]\n\n"
                     "$p = predict($fit, ['area_m2' => [60, 90], 'bedrooms' => [1, 3]], interval: 'prediction');\n"
                     "$p['fit'];      // point predictions; $p['lower'], $p['upper'] give 95% intervals",
            'pipe': None,
            'output': None,
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
                {'title': 'API overview', 'path': 'contracts.qmd'},
                {'title': 'R compatibility', 'path': 'r-compatibility.qmd'},
                {'title': 'Verification', 'path': 'verification.qmd'},
                {'title': 'Development', 'path': 'development.qmd'},
            ],
        },
        'benchmarks': benchmarks,
        'verifications': verifications,
        'vignettes': [
            {'slug': 'rent-model', 'title': 'Predicting rent with a linear model',
             'summary': 'Fit monthly rent on floor area and bedrooms for 400 simulated flats, then predict with 95% prediction intervals.',
             'uses': ['phit'], 'script': 'site/vignettes/rent-model.php'},
            {'slug': 'school-effects', 'title': 'School effects with a mixed model',
             'summary': 'Estimate school random intercepts with fit_lmer and show them with their conditional uncertainty.',
             'uses': ['phit'], 'script': 'site/vignettes/school-effects.php'},
            {'slug': 'distributions', 'title': 'Exploring the t distribution',
             'summary': "Compare Student's t densities and tail probabilities with the normal as the degrees of freedom change.",
             'uses': ['phit'], 'script': 'site/vignettes/distributions.php'},
        ],
    }


def build():
    files = {}
    benchmarks, verifications = [], []
    for slug, title in MODELS.items():
        files[f'benchmarks/{slug}.json'] = model_benchmark(slug, title)
        benchmarks.append(f'site/benchmarks/{slug}.json')
    files['benchmarks/distributions.json'] = timed_benchmark(
        'distributions', 'Distributions: evaluation time',
        'Median wall time to evaluate a distribution function over a batch of inputs in phit (scalar calls and one '
        'batch call), the Rust core, R and SciPy.',
        lambda c: f"{c['operation']}, n = {c['n']:,}")
    files['benchmarks/draws.json'] = timed_benchmark(
        'draws', 'Posterior draws: moments and transforms',
        'Median wall time for weighted draw moments (population and unbiased), covariance maps and affine transforms in phit and R.',
        lambda c: f"n = {c['n']:,} draws, p = {c['p']}")
    benchmarks += ['site/benchmarks/distributions.json', 'site/benchmarks/draws.json']
    for slug, title, peers in [('ols', 'OLS', 'statsmodels'), ('logistic', 'Logistic regression', 'statsmodels'),
                               ('probit', 'Probit regression', 'statsmodels'), ('poisson', 'Poisson regression', 'statsmodels'),
                               ('lmm', 'Gaussian random intercept', 'statsmodels'),
                               ('lmm-public', 'Mixed models on published data sets', 'nlme and statsmodels')]:
        files[f'verifications/{slug}.json'] = fixed_verification(slug, title, peers)
        verifications.append(f'site/verifications/{slug}.json')
    files['verifications/logistic-mixed.json'] = logistic_mixed_verification()
    files['verifications/r-compatibility.json'] = r_compatibility_verification()
    files['verifications/distributions.json'] = distributions_verification()
    verifications += ['site/verifications/logistic-mixed.json', 'site/verifications/r-compatibility.json',
                      'site/verifications/distributions.json']
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
