"""Reproducible scale checks and API-level regression benchmarks."""
import argparse
from datetime import datetime, timezone
import hashlib
import io
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess
import sys
import tempfile
import time
from contextlib import redirect_stdout

import numpy as np

from machine import extension as machine_extension, machine, memory_command, peak_memory

ROOT = Path(__file__).resolve().parents[2]
HERE = Path(__file__).resolve().parent
THREAD_ENV = {name: '1' for name in ('OPENBLAS_NUM_THREADS', 'OMP_NUM_THREADS',
              'MKL_NUM_THREADS', 'VECLIB_MAXIMUM_THREADS', 'BLIS_NUM_THREADS')}


def dataset(n, p, path, model):
    rng = np.random.default_rng(20261011)
    if model in ('logistic','probit','poisson'):
        hours = rng.uniform(1,12,n)
        score = np.clip(45+3*hours+rng.normal(0,8,n),20,95)
        x = np.column_stack([np.ones(n),hours,score,rng.normal(size=(n,p-3))])
        beta = np.array([-3.28,0.22,0.018,*([0.15]*(p-3))])
        distribution = 'Bernoulli(sigmoid(X beta))'
        if model == 'probit':
            from scipy.special import ndtr
            beta = np.array([-1.9,0.13,0.011,*([0.08]*(p-3))])
            probability = ndtr(x@beta)
            distribution = 'Bernoulli(Phi(X beta))'
        elif model == 'poisson':
            hours = rng.integers(80,301,n)
            score = np.clip(hours/50+rng.normal(0,0.8,n),2,10)
            x = np.column_stack([np.ones(n),hours,score,rng.normal(size=(n,p-3))])
            beta = np.array([0.1,0.005,-0.12,*([0.1]*(p-3))])
            probability = np.exp(x@beta)
            distribution = 'Poisson(exp(X beta))'
        else:
            probability = 1/(1+np.exp(-(x@beta)))
        y = rng.poisson(probability) if model == 'poisson' else rng.binomial(1,probability)
        header = ['y','intercept','study_hours','practice_score']+[f'preparation_score_{j}' for j in range(p-3)]
        if model == 'poisson':
            header = ['y','intercept','visitors','staff']+[f'additional_score_{j}' for j in range(p-3)]
        np.savetxt(path,np.column_stack([y,x]),delimiter=',',fmt='%.17g',header=','.join(header),comments='')
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        return {'seed':20261011,'sha256':digest,'true_coefficients':beta.tolist(),
                'outcome_distribution':distribution,
                'sample_correlations':np.corrcoef([hours,score,y]).tolist(),
                'correlation_order':(['visitors','staff','calls'] if model=='poisson' else ['study_hours','practice_score','passed'])}
    if model == 'lmm':
        group_count = max(2,min(100,n//20))
        groups = np.arange(n)%group_count
        hours = rng.uniform(1,12,n)
        prior = np.clip(40+3*hours+rng.normal(0,8,n),20,95)
        x = np.column_stack([np.ones(n),hours,prior,rng.normal(size=(n,p-3))])
        beta = np.array([50.,1.8,0.2,*([0.5]*(p-3))])
        effects = rng.normal(0,6,group_count)
        y = x@beta + effects[groups] + rng.normal(0,5,n)
        header=['y','group','intercept','study_hours','prior_score']+[f'extra_{j}' for j in range(p-3)]
        np.savetxt(path,np.column_stack([y,groups,x]),delimiter=',',fmt='%.17g',header=','.join(header),comments='')
        return dict(seed=20261011,sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                    true_coefficients=beta.tolist(),residual_sd=5.,random_intercept_sd=6.,n_groups=group_count,
                    sample_correlations=np.corrcoef([hours,prior,y]).tolist(),correlation_order=['study_hours','prior_score','score'])
    area = rng.uniform(35, 110, n)
    bedrooms = np.clip(np.rint((area - 15) / 25 + rng.normal(0, 0.6, n)), 1, 4)
    x = np.column_stack([np.ones(n), area, bedrooms,
                         rng.normal(size=(n, p - 3))])
    beta = np.array([650., 18., 60., *([0.5] * (p - 3))])
    y = x @ beta + rng.normal(0, 180, n)
    header = ['y', 'intercept', 'area_m2', 'bedrooms'] + [f'amenity_score_{j}' for j in range(p - 3)]
    np.savetxt(path, np.column_stack([y, x]), delimiter=',', fmt='%.17g',
               header=','.join(header), comments='')
    digest = hashlib.sha256()
    with path.open('rb') as stream:
        for chunk in iter(lambda: stream.read(1024 * 1024), b''):
            digest.update(chunk)
    return {'seed': 20261011, 'sha256': digest.hexdigest(), 'true_coefficients': beta.tolist(),
            'error_sd': 180, 'sample_correlations': np.corrcoef([area, bedrooms, y]).tolist(),
            'correlation_order': ['area_m2', 'bedrooms', 'monthly_rent']}


def worker(command, env):
    # Memory covers the whole process, not just the fit.
    command = [*memory_command(), *command]
    start = time.perf_counter()
    completed = subprocess.run(command, env=env, capture_output=True, text=True,
                               timeout=600, check=True)
    elapsed = time.perf_counter() - start
    result = json.loads(completed.stdout)
    result.update(peak_memory(completed.stderr))
    result.update(process_seconds=elapsed,
                  median_fit_seconds=statistics.median(result['fit_seconds']),
                  min_fit_seconds=min(result['fit_seconds']))
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--model', choices=['ols','logistic','probit','poisson','lmm'], default='ols')
    parser.add_argument('--cases', default='1000:3,100000:3,1000000:3,100000:11',
                        help='Comma-separated n:p, with p INCLUDING the intercept (p >= 3)')
    parser.add_argument('--repeats', type=int, default=5)
    parser.add_argument('--output', type=Path)
    args = parser.parse_args()
    if args.output is None:
        args.output = ROOT / f'docs/benchmarks/{args.model}-results.json'
    if args.repeats < 1:
        parser.error('repeats must be positive')
    cases = [tuple(map(int, value.split(':'))) for value in args.cases.split(',')]
    if any(p < 3 or n <= p for n, p in cases):
        parser.error('Require n > p >= 3')
    extension = machine_extension()
    if not extension.is_file():
        raise FileNotFoundError(f'Build the RELEASE extension first: {extension}')
    env = {**os.environ, **THREAD_ENV}
    config = io.StringIO()
    with redirect_stdout(config):
        np.show_config()
    report = {'recorded_at': datetime.now(timezone.utc).isoformat(),
              'model': args.model, **machine(),
              'python': platform.python_version(), 'numpy_config': config.getvalue(),
              'thread_environment': THREAD_ENV, 'repeats': args.repeats, 'warmups': 1,
              'extension_sha256': hashlib.sha256(extension.read_bytes()).hexdigest(),
              'rust': subprocess.check_output(['rustc', '--version'], text=True).strip(),
              'tolerance': {'absolute': 1e-7 if args.model=='lmm' else 1e-9, 'relative': 1e-6 if args.model=='lmm' else 1e-7}, 'cases': []}
    if args.model == 'lmm':
        report['tolerance']['quantity_relative'] = {
            key:1e-5 for key in ['covariance','std_errors','z_statistics','residual_variance',
                                 'random_intercept_variance','sigma','icc']}
        report['tolerance']['quantity_relative']['log_likelihood'] = 1e-9
        report['tolerance']['quantity_relative']['aic'] = 1e-9
    scratch = ROOT / '.context'
    scratch.mkdir(exist_ok=True)
    with tempfile.TemporaryDirectory(prefix='ols-benchmark-', dir=scratch) as directory:
        for n, p in cases:
            data = Path(directory) / 'data.csv'
            generated = dataset(n, p, data, args.model)
            print(f'n={n:,}, p={p}: generated shared observations', flush=True)
            results = {}
            commands = {
                'R': [os.environ.get('RSCRIPT', 'Rscript'), str(HERE / f'{args.model}.R')],
                'statsmodels': [sys.executable, str(HERE / f'{args.model}.py')],
                'PHP': [os.environ.get('PHP', 'php'), '-n', '-d', 'memory_limit=-1',
                        '-d', f'extension={extension}', str(HERE / 'ols.php')],
            }
            # Alternate process order to reduce consistently favoring one runtime.
            order = list(commands) if len(report['cases']) % 2 == 0 else list(reversed(commands))
            for name in order:
                results[name] = worker([*commands[name], str(data), str(args.repeats), args.model], env)
                print(f'  {name}: median full fit {results[name]["median_fit_seconds"]:.6f}s', flush=True)
            errors = {}
            reference = results['R']['result']
            for name in ['statsmodels', 'PHP']:
                assert results[name]['sample_indices'] == results['R']['sample_indices']
                errors[name] = {}
                for key, expected in reference.items():
                    actual = results[name]['result'][key]
                    np.testing.assert_allclose(actual, expected, atol=report["tolerance"]["absolute"], rtol=report["tolerance"].get("quantity_relative",{}).get(key,report["tolerance"]["relative"]),
                                               err_msg=f'n={n},p={p}/{name}/{key}')
                    errors[name][key] = float(np.max(np.abs(np.asarray(actual) - expected)))
                assert results[name]['result']['df_residual'] == n - p
            if args.model != 'ols':
                assert results['PHP']['result']['converged']
                if args.model != 'lmm':
                    assert 1 <= results['PHP']['result']['iterations'] <= 100
            report['cases'].append(dict(n=n, p=p, data=generated, implementations=results,
                                        max_absolute_errors_vs_R=errors, agreement_passed=True))
            print('  Numerical agreement passed.', flush=True)
    # Publish a complete report only after every case passes verification.
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2, allow_nan=False) + '\n')
    print(f'Wrote {args.output}', flush=True)


if __name__ == '__main__':
    main()
