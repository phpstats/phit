#!/usr/bin/env python3
"""Run the PHP test suites against the release extension and record each suite's status and summary.

Writes docs/verifications/test-suites-results.json, which scripts/site-export.py turns into a verification record.
Run from the repository root after building the release extension:
    PHP=$(command -v php) python3 docs/verifications/run_test_suites.py
"""
import json
import os
import platform
import subprocess
import sys
import time
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tests/benchmarks'))
from machine import extension  # noqa: E402

# The suites scripts/check runs, in its order. csv-phibble needs a phibble checkout (PHIBBLE_AUTOLOAD).
SUITES = ['ols', 'logistic', 'probit', 'poisson', 'aliases', 'formula', 'formula-mixed', 'predict', 'lmm',
          'r-compatibility', 'lmm-expanded', 'logistic-mixed', 'stability', 'intercept', 'distributions',
          'descriptive', 'smoothing', 'optimize', 'rng', 'draws', 'csv', 'csv-conformance', 'csv-phibble',
          'extreme-inputs']


def main():
    php = os.environ.get('PHP', 'php')
    ext = extension()
    results = []
    for suite in SUITES:
        start = time.perf_counter()
        done = subprocess.run([php, '-n', '-d', f'extension={ext}', str(ROOT / 'tests' / f'{suite}.php')],
                              capture_output=True, text=True, cwd=ROOT)
        lines = [line for line in done.stdout.splitlines() if line.strip()]
        summary = lines[-1].strip() if lines else ''
        skipped = done.returncode == 0 and 'skipped' in summary
        results.append({'suite': suite, 'command': f'php -n -d extension=… tests/{suite}.php',
                        'status': 'skipped' if skipped else ('pass' if done.returncode == 0 else 'fail'),
                        'summary': summary, 'returncode': done.returncode,
                        'error': done.stderr.strip()[-2000:] if done.returncode else None,
                        'seconds': round(time.perf_counter() - start, 3)})
        print(f"{suite}: {results[-1]['status']}: {summary}", flush=True)
    commit = subprocess.run(['git', 'rev-parse', '--short', 'HEAD'], capture_output=True, text=True, cwd=ROOT)
    report = {
        'recorded_at': datetime.now(timezone.utc).isoformat(),
        'commit': commit.stdout.strip() or None,
        'php': subprocess.check_output([php, '-n', '-r', 'echo PHP_VERSION;'], text=True),
        'phit': subprocess.check_output([php, '-n', '-d', f'extension={ext}', '-r', 'echo phpversion("phit");'],
                                        text=True),
        'os': platform.platform(),
        'suites': results,
    }
    target = ROOT / 'docs/verifications/test-suites-results.json'
    target.write_text(json.dumps(report, indent=2) + '\n')
    print(f'Wrote {target.relative_to(ROOT)}')
    sys.exit(1 if any(r['status'] == 'fail' for r in results) else 0)


if __name__ == '__main__':
    main()
