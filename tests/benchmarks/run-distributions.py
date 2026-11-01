#!/usr/bin/env python3
"""Compare numerical summaries BEFORE timing PHP scalar/batch, Rust, R and SciPy.

Build: cargo build -p phit --release --locked
       cargo build -p stats-core --release --locked --example distribution_bench
Run: PHP=$(command -v php) python3 tests/benchmarks/run-distributions.py
"""
import argparse
from contextlib import redirect_stdout
from datetime import datetime, timezone
import hashlib
import io
import json
import math
import os
from pathlib import Path
import platform
import re
import statistics
import subprocess
import sys
import tempfile
import time
import numpy as np
import scipy

from machine import extension as machine_extension, machine, memory_command, peak_memory

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
OPERATIONS = ['normal_cdf','normal_sf','t_quantile','binomial_logpmf','poisson_logpmf',
              'mean','variance','beta_quantile','chisquare_cdf']
THREADS = {key: '1' for key in ['OPENBLAS_NUM_THREADS','OMP_NUM_THREADS','MKL_NUM_THREADS',
                               'VECLIB_MAXIMUM_THREADS','BLIS_NUM_THREADS']}


def dataset(op, n):
    rng = np.random.default_rng(20261012)
    if op == 'normal_cdf':
        x = rng.uniform(-8, 8, n)
    elif op == 'normal_sf':
        x = rng.uniform(0, 12, n)
    elif op in ('t_quantile','beta_quantile'):
        x = rng.uniform(.001, .999, n)
    elif op == 'binomial_logpmf':
        x = rng.integers(0, 101, n)
    elif op == 'poisson_logpmf':
        x = rng.integers(0, 41, n)
    elif op == 'chisquare_cdf':
        x = 1e6 + rng.uniform(-5, 5, n)*math.sqrt(2e6)
    else:
        x = rng.normal(10, 2, n)
    batch = max(10, 10000000//n) if op in ('mean','variance') else (100 if n == 1000 else 1)
    return dict(operation=op, values=x.tolist(), batch=batch)


def agreement(actual, reference, op, implementation):
    assert actual['length'] == reference['length']
    errors = dict(agreement_passed=True, max_sample_absolute=0., max_sample_tolerance_fraction=0., max_aggregate_relative=0.)
    for key in ('sum','sum_squares'):
        a, b = actual[key], reference[key]
        passed = math.isfinite(a) and abs(a-b) <= 1e-9 + 1e-10*abs(b)
        errors['agreement_passed'] &= passed
        if implementation != 'SciPy':
            assert passed, (key, a, b)
        errors['max_aggregate_relative'] = max(errors['max_aggregate_relative'], abs(a-b)/max(1., abs(b)))
    pairs = zip(actual['sample']+[actual['min'],actual['max']], reference['sample']+[reference['min'],reference['max']])
    for a, b in pairs:
        # Apply the same tolerance to every implementation, recording a peer
        # discrepancy without weakening the native correctness gate.
        relative = 1e-12
        floor = 1e-15 if op.endswith('quantile') else 1e-300
        fraction = abs(a-b)/(floor + relative*abs(b))
        passed = math.isfinite(a) and fraction <= 1.
        errors['agreement_passed'] &= passed
        if implementation != 'SciPy':
            assert passed, (op, implementation, a, b, fraction)
        errors['max_sample_absolute'] = max(errors['max_sample_absolute'], abs(a-b))
        errors['max_sample_tolerance_fraction'] = max(errors['max_sample_tolerance_fraction'], fraction)
    return errors


def worker(command, env, timed):
    command = [*memory_command(), *command]
    start = time.perf_counter()
    completed = subprocess.run(command, env=env, capture_output=True, text=True, timeout=1200)
    if completed.returncode:
        raise RuntimeError(f'{command}: {completed.stderr}\n{completed.stdout}')
    result = json.loads(completed.stdout)
    result['process_seconds'] = time.perf_counter()-start
    result.update(peak_memory(completed.stderr))
    if timed:
        assert all(t > 0 for t in result['seconds']), 'increase small-case batching for timer resolution'
        result['median_seconds'] = statistics.median(result['seconds'])
        result['minimum_seconds'] = min(result['seconds'])
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--sizes', default='1000,100000,1000000')
    parser.add_argument('--operations', default=','.join(OPERATIONS))
    parser.add_argument('--repeats', type=int, default=5)
    parser.add_argument('--resume', action='store_true', help='Resume matching partial results after an interrupted measurement')
    parser.add_argument('--output', type=Path, default=ROOT/'docs/benchmarks/distributions-results.json')
    args = parser.parse_args()
    sizes = list(map(int, args.sizes.split(',')))
    operations = args.operations.split(',')
    if min(sizes) < 2 or args.repeats < 3 or any(op not in OPERATIONS for op in operations):
        parser.error('sizes >= 2, repeats >= 3 and known operations required')
    php = os.environ.get('PHP', 'php')
    extension = machine_extension()
    native = ROOT/'target/release/examples/distribution_bench'
    if not extension.is_file() or not native.is_file():
        parser.error('Build the release extension and distribution_bench example first')
    env = {**os.environ, **THREADS}
    php_command = [php, '-n', '-d', 'memory_limit=-1', '-d', f'extension={extension}', str(HERE/'distributions.php')]
    commands = dict(PHP_scalar=php_command, PHP_batch=php_command, Rust=[str(native)],
                    R=[os.environ.get('RSCRIPT', 'Rscript'), str(HERE/'distributions.R')],
                    SciPy=[sys.executable, str(HERE/'distributions.py')])
    config = io.StringIO()
    with redirect_stdout(config):
        np.show_config()
    report = dict(recorded_at=datetime.now(timezone.utc).isoformat(), **machine(),
        python=platform.python_version(), numpy=np.__version__, scipy=scipy.__version__, numpy_config=config.getvalue(),
        rust=subprocess.check_output(['rustc','--version'], text=True).strip(), thread_environment=THREADS,
        extension_sha256=hashlib.sha256(extension.read_bytes()).hexdigest(), rust_worker_sha256=hashlib.sha256(native.read_bytes()).hexdigest(),
        repeats=args.repeats, warmups=1, seed=20261012,
        timing='Preloaded numeric inputs; complete output vector allocated and filled each call, including PHP input/output conversion and validation. Startup, JSON parsing and summaries excluded. Peak RSS and process time cover the entire timed worker.',
        verification='17 evenly spaced outputs plus min/max at fixture tolerances; all outputs consumed by sum and sum_squares at 1e-9 + 1e-10*abs(reference). Quantiles 1e-15 + 1e-12 relative; others 1e-300 + 1e-12 relative. Native agreement with R is required; SciPy discrepancies are explicitly recorded without relaxing the native gate. Independent mpmath fixtures separately adjudicate R disagreements.',
        cases=[])
    report['complete'] = False
    previous = {}
    if args.resume and args.output.exists():
        saved = json.loads(args.output.read_text())
        for key in ('extension_sha256','rust_worker_sha256','repeats','seed','thread_environment'):
            assert saved[key] == report[key], f'Cannot resume after {key} changed'
        previous = {(c['operation'],c['n']):c for c in saved['cases']}
    def save():
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(report, indent=2, allow_nan=False)+'\n')
    with tempfile.TemporaryDirectory(prefix='distribution-bench-', dir=ROOT/'.context') as directory:
        paths = []
        for op in operations:
            for n in sizes:
                data = dataset(op, n)
                path = Path(directory)/f'{op}-{n}.json'
                path.write_text(json.dumps(data, separators=(',', ':'), allow_nan=False))
                paths.append(path)
                digest = hashlib.sha256(path.read_bytes()).hexdigest()
                old = previous.get((op,n))
                if old is not None and old['input_sha256'] == digest:
                    report['cases'].append(old)
                    print(f'{op} n={n}: saved numerical verification retained', flush=True)
                    continue
                names = [name for name in commands if not (op in ('mean','variance') and name == 'PHP_scalar')]
                outputs = {name: worker([*commands[name], str(path), 'verify', str(args.repeats), name], env, False) for name in names}
                errors = {name: agreement(output['result'], outputs['R']['result'], op, name) for name, output in outputs.items()}
                report['cases'].append(dict(operation=op, n=n, batch=data['batch'], input_sha256=digest,
                                           agreement_passed=True, verification=outputs, agreement=errors, implementations={}))
                save()
                peer = 'agrees' if errors['SciPy']['agreement_passed'] else 'exceeds comparison tolerance (recorded)'
                print(f'{op} n={n}: native numerical comparisons passed; SciPy {peer}', flush=True)
        print('All native numerical verification passed; peer discrepancies recorded. Starting timing phase.', flush=True)
        # Alternate PHP scalar/batch execution order across cases to reduce bias.
        for index, (case, path) in enumerate(zip(report['cases'], paths)):
            names = list(case['verification'])
            if index % 2:
                names.reverse()
            for name in names:
                if name in case['implementations']:
                    continue
                result = worker([*commands[name], str(path), 'time', str(args.repeats), name], env, True)
                agreement(result['result'], case['verification']['R']['result'], case['operation'], name)
                case['implementations'][name] = result
                save()
            print(f'{case["operation"]} n={case["n"]}: '+str({name:r['median_seconds'] for name,r in case['implementations'].items()}), flush=True)
    report['complete'] = True
    save()
    print(f'Saved verification, raw timings and peak RSS: {args.output}')


if __name__ == '__main__':
    main()
