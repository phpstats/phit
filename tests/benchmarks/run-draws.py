#!/usr/bin/env python3
"""Verify full outputs against R before API-level release timings (no NumPy).

Usage: PHP=$(command -v php) python3 tests/benchmarks/run-draws.py
Development reference dependency: R + jsonlite. No extension runtime dependency.
"""
import argparse
from datetime import datetime, timezone
import hashlib
import json
import math
import os
from pathlib import Path
import platform
import random
import statistics
import subprocess
import tempfile

from machine import extension as machine_extension, machine

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
THREADS = {name: '1' for name in ('OPENBLAS_NUM_THREADS', 'OMP_NUM_THREADS',
           'MKL_NUM_THREADS', 'VECLIB_MAXIMUM_THREADS', 'BLIS_NUM_THREADS')}


def covariance(p, diagonal, sign):
    lower = [[(diagonal + .03*i if i == j else sign * .15 / (i-j) if i > j else 0.)
              for j in range(p)] for i in range(p)]
    return [[sum(lower[i][k] * lower[j][k] for k in range(p)) for j in range(p)] for i in range(p)]


def dataset(n, p):
    rng = random.Random(20261012)
    x = []
    for _ in range(n):
        common = rng.gauss(0, 1)
        x.append([.1*j + .4*common + rng.gauss(0, 1) for j in range(p)])
    w = [0. if i % 11 == 0 else 1. + (i % 7) for i in range(n)]
    a = [[(-1.5 if i == 0 else 1. + .02*i) if i == j else .1 / (1 + abs(i-j))
          for j in range(p)] for i in range(p)]
    # R's elapsed timer has millisecond resolution: batch enough calls to avoid
    # zero readings on the small workloads. Report per-call time for both APIs.
    batches = 1000 if n*p < 50000 else 100 if n*p < 100000 else 10
    return dict(draws=x, weights=w, source_covariance=covariance(p, 1., 1),
                target_covariance=covariance(p, 1.5, -1), matrix=a, shift=[.2*j for j in range(p)],
                batches=dict(population=batches, unbiased=batches, covariance_map=1000, affine=batches))


def agreement(actual, expected):
    errors = dict(values=0, max_absolute=0., max_tolerance_fraction=0.)

    def walk(a, b):
        if isinstance(b, dict):
            assert a.keys() == b.keys(), 'result keys differ'
            for k in b:
                walk(a[k], b[k])
        elif isinstance(b, list):
            assert len(a) == len(b), 'result lengths differ'
            for v, r in zip(a, b):
                walk(v, r)
        else:
            assert math.isfinite(a) and math.isfinite(b), 'nonfinite result'
            delta = abs(a-b)
            fraction = delta / (1e-11 + 1e-11*abs(b))
            assert fraction <= 1., f'{a} != {b}: tolerance fraction {fraction}'
            errors['values'] += 1
            errors['max_absolute'] = max(errors['max_absolute'], delta)
            errors['max_tolerance_fraction'] = max(errors['max_tolerance_fraction'], fraction)

    walk(actual, expected)
    return errors


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--repeats', type=int, default=5)
    parser.add_argument('--output', type=Path, default=ROOT/'docs/benchmarks/draws-results.json')
    args = parser.parse_args()
    if args.repeats < 2:
        parser.error('--repeats must be at least 2')
    php = os.environ.get('PHP', 'php')
    rscript = os.environ.get('RSCRIPT', 'Rscript')
    extension = str(machine_extension())
    env = {**os.environ, **THREADS}
    commands = dict(PHP=[php, '-n', '-d', 'memory_limit=-1', '-d', f'extension={extension}', str(HERE/'draws.php')],
                    R=[rscript, str(HERE/'draws.R')])

    def worker(name, path, mode):
        result = subprocess.run([*commands[name], str(path), mode, str(args.repeats)],
                                env=env, text=True, capture_output=True, check=True, timeout=600)
        return json.loads(result.stdout)

    report = dict(recorded_at=datetime.now(timezone.utc).isoformat(), **machine(),
                  machine=platform.machine(), python=platform.python_version(),
                  rust=subprocess.check_output(['rustc', '--version'], text=True).strip(),
                  threads=THREADS, seed=20261012, repeats=args.repeats, warmups=1,
                  tolerance='1e-11 + 1e-11*abs(reference)', extension=extension,
                  boundaries='Preloaded inputs; complete result computation/materialization; PHP array conversion and native validation included. Startup, JSON I/O and reference comparisons excluded. Per-call seconds from batches.',
                  cases=[])
    with tempfile.TemporaryDirectory(prefix='phit-draw-bench-') as directory:
        paths = []
        for n, p in [(1000, 4), (4000, 16), (10000, 32), (100000, 8)]:
            path = Path(directory)/f'{n}-{p}.json'
            inputs = dataset(n, p)
            path.write_text(json.dumps(inputs, separators=(',', ':')))
            paths.append(path)
            actual, expected = worker('PHP', path, 'verify'), worker('R', path, 'verify')
            errors = {key: agreement(actual[key], expected[key]) for key in expected}
            report['cases'].append(dict(n=n, p=p, input_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                                        batches=inputs['batches'], agreement_passed=True, agreement=errors))
            print(f'{n} x {p}: full numerical agreement passed; max absolute error '
                  f'{max(v["max_absolute"] for v in errors.values()):.3g}', flush=True)
        print('All numerical comparisons passed. Starting timings.', flush=True)
        for path, case in zip(paths, report['cases']):
            case['implementations'] = {}
            for name in ('R', 'PHP'):
                result = worker(name, path, 'time')
                result['median_seconds'] = {k: statistics.median(v) for k, v in result['seconds'].items()}
                case['implementations'][name] = result
                print(f'{case["n"]} x {case["p"]} {name}: {result["median_seconds"]}', flush=True)
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(f'Saved raw agreement and timings: {args.output}')


if __name__ == '__main__':
    main()
