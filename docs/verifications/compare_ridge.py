"""Compare native and pure PHP ridge against saved public R API outputs."""
import json
import os
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[2]
PHP = os.environ.get('PHP', '/opt/homebrew/opt/php@8.4/bin/php')
EXT = os.environ.get('STATS_EXTENSION', str(ROOT / 'target/release/libphit.dylib'))
POLY = Path(os.environ.get('PHIT_POLYFILL', str(ROOT.parent / 'phit-polyfill')))
fixtures = json.loads((ROOT / 'tests/fixtures/ridge-r.json').read_text())
commands = {
    'native': [PHP, '-n', '-d', f'extension={EXT}', str(ROOT / 'docs/verifications/ridge.php')],
    'polyfill': [PHP, '-d', f'auto_prepend_file={POLY}/vendor/autoload.php', str(ROOT / 'docs/verifications/ridge.php')],
}
results = {}
for implementation, command in commands.items():
    actual = json.loads(subprocess.check_output(command, cwd=ROOT, text=True))
    records = []
    for case in [*fixtures['fits'], *fixtures['cv']]:
        case = dict(case)
        if 'cv_name' in case: case.update(name='cv/'+case['cv_name'], expected=case['expected_cv'])
        errors = []
        def compare(a, b, path=''):
            if isinstance(b, dict):
                for key, value in b.items(): compare(a[key], value, f'{path}/{key}')
            elif isinstance(b, list):
                assert len(a) == len(b), path
                for index, value in enumerate(b): compare(a[index], value, f'{path}/{index}')
            elif b is None or isinstance(b, str):
                assert a == b, path
            else:
                error = abs(a - b)
                tolerance = 1e-7 + 2e-6 * abs(b)
                assert error <= tolerance, (implementation, case['name'], path, a, b)
                errors.append((error, error / tolerance, path))
        compare(actual[case['name']], case['expected'])
        largest = max(errors)
        records.append({'name': case['name'], 'passed': True, 'comparisons': len(errors),
                        'max_absolute_error': largest[0], 'max_error_quantity': largest[2],
                        'max_tolerance_fraction': max(row[1] for row in errors),
                        'result': {key: actual[case['name']][key] for key in case['expected']}})
    results[implementation] = records
report = {'schema_version': 1, 'title': 'Gaussian ridge and cross-validation', 'function': 'fit_ridge, cv_ridge',
          'reference': fixtures['reference'], 'tolerance': {'absolute': 1e-7, 'relative': 2e-6},
          'cases': results, 'passed': True,
          'references': {**{c['name']: c['expected'] for c in fixtures['fits']}, **{'cv/'+c['cv_name']: c['expected_cv'] for c in fixtures['cv']}},
          'differences': ['Explicit Gaussian lambda is divided by the weighted response scale to match glmnet output.', 'Conditional covariance uses RSS/(n-df); lmridge uses a different residual-variance denominator, so covariance per sigma squared is compared separately.', 'Default lambda paths are phit policy; explicit paths are used for glmnet comparisons.'],
          'reproduce': 'Rscript docs/verifications/ridge.R && python3 docs/verifications/compare_ridge.py'}
(ROOT / 'docs/verifications/ridge-results.json').write_text(json.dumps(report, indent=2) + '\n')
print(f'ridge: {len(fixtures["fits"])+len(fixtures["cv"])} cases per implementation passed')
