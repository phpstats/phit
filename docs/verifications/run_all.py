#!/usr/bin/env python3
"""Run every verification comparison and the PHP test suites, and record the run's environment.

Writes the *-results.json files the comparisons produce, test-suites-results.json, and run-environment.json
(date, source commit, OS, PHP, and phit of this run). scripts/site-export.py reads them.
Run from the repository root with the notebook environment and a release build:
    PHP=$(command -v php) .context/notebooks-venv/bin/python docs/verifications/run_all.py
"""
import json
import os
import platform
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'tests/benchmarks'))
from machine import extension  # noqa: E402

COMPARISONS = ['ols', 'logistic', 'probit', 'poisson', 'lmm', 'logistic_mixed', 'r_compatibility', 'distributions']


def main():
    php = os.environ.get('PHP', 'php')
    env = {**os.environ, 'PHP': php, 'STATS_EXTENSION': str(extension())}
    commit = subprocess.run(['git', 'rev-parse', '--short', 'HEAD'], capture_output=True, text=True, cwd=ROOT)
    dirty = subprocess.run(['git', 'status', '--porcelain', '--untracked-files=no'], capture_output=True, text=True,
                           cwd=ROOT).stdout.strip()
    runs = [[sys.executable, f'docs/verifications/compare_{name}.py'] for name in COMPARISONS]
    runs.insert(COMPARISONS.index('lmm') + 1, [sys.executable, 'docs/verifications/compare_lmm.py', '--public'])
    runs.append([sys.executable, 'docs/verifications/run_test_suites.py'])
    for command in runs:
        print('$', ' '.join(command[1:]), flush=True)
        subprocess.run(command, check=True, cwd=ROOT, env=env)
    report = {
        'recorded_at': datetime.now(timezone.utc).isoformat(),
        'commit': commit.stdout.strip() + ('+changes' if dirty else ''),
        'os': platform.platform(),
        'php': subprocess.check_output([php, '-n', '-r', 'echo PHP_VERSION;'], text=True),
        'phit': subprocess.check_output([php, '-n', '-d', f'extension={extension()}', '-r',
                                         'echo phpversion("phit");'], text=True),
    }
    (ROOT / 'docs/verifications/run-environment.json').write_text(json.dumps(report, indent=2) + '\n')
    print('Wrote docs/verifications/run-environment.json')


if __name__ == '__main__':
    main()
