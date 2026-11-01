"""Runs the model workload benchmarks and writes site/benchmarks/{ols,mixed}-workload.json.

    python bench/run.py --polyfill /path/to/phit-polyfill [--workload ols|mixed|all] [--repeats 7]

Every run is a fresh process. Each implementation runs once as a warm-up, then the implementations alternate for
--repeats rounds, in a rotated order each round. bench/README.md describes the method and the requirements.
"""
import argparse
from datetime import datetime, timezone
import json
import os
from pathlib import Path
import platform
import re
import statistics
import subprocess
import sys
import textwrap

ROOT = Path(__file__).resolve().parents[1]
HERE = Path(__file__).resolve().parent
DATA = ROOT / '.context' / 'bench-data'
# One BLAS/OpenMP thread in every runtime; phit and the polyfill are single-threaded.
THREADS = {name: '1' for name in ('OPENBLAS_NUM_THREADS', 'OMP_NUM_THREADS', 'MKL_NUM_THREADS',
                                  'VECLIB_MAXIMUM_THREADS', 'BLIS_NUM_THREADS')}
IMPLEMENTATIONS = ['phit', 'phit-polyfill', 'R', 'statsmodels']
LANGUAGES = {'phit': 'php', 'phit-polyfill': 'php', 'R': 'r', 'statsmodels': 'python'}

# The timed part of each script, verbatim; run() checks that each script contains it.
CODE = {
    'ols': {
        'phit': "$fit = fit_ols('y ~ x1 + x2 + x3 + x4 + x5 + x6 + x7 + x8 + x9 + x10', $data);",
        'R': 'fit <- lm(y ~ x1 + x2 + x3 + x4 + x5 + x6 + x7 + x8 + x9 + x10, data = d)\n  table <- coef(summary(fit))',
        'statsmodels': "fit = smf.ols('y ~ x1 + x2 + x3 + x4 + x5 + x6 + x7 + x8 + x9 + x10', data=d).fit()\n"
                       "    coefficients, standard_errors = fit.params, fit.bse",
    },
    'mixed': {
        'phit': "$fit = fit_mixed_gaussian('y ~ x1 + x2 + x3 + (1 | g)', $data);",
        'R': 'fit <- lmer(y ~ x1 + x2 + x3 + (1 | g), data = d, REML = TRUE)\n  table <- coef(summary(fit))',
        'statsmodels': "fit = smf.mixedlm('y ~ x1 + x2 + x3', d, groups=d['g']).fit(reml=True)\n"
                       "    coefficients, standard_errors = fit.fe_params, fit.bse_fe",
    },
}
SCRIPTS = {'phit': 'fit.php', 'phit-polyfill': 'fit.php', 'R': 'fit.R', 'statsmodels': 'fit.py'}

WORKLOADS = {
    'ols': {
        'title': 'OLS workload: phit, phit-polyfill, R and statsmodels',
        'description': 'Elapsed time and peak memory to fit a linear model by least squares from a formula, '
                       'with coefficients and standard errors, in the phit extension, phit-polyfill, R lm() and '
                       'statsmodels OLS.',
        'case': '1,000,000 rows, 10 predictors',
        'workload': 'OLS, 1,000,000 rows, 10 predictors',
        'convergence': 'OLS has no iterations. phit and phit-polyfill solve by column-scaled SVD. R lm() uses a '
                       'Householder QR (LINPACK dqrls). statsmodels OLS.fit() uses its default method, "pinv": '
                       'the Moore-Penrose pseudoinverse by SVD.',
    },
    'mixed': {
        'title': 'Gaussian mixed-model workload: phit, phit-polyfill, lme4 and statsmodels',
        'description': 'Elapsed time and peak memory to fit a Gaussian random-intercept model by REML from a '
                       'formula, with fixed-effect coefficients and standard errors, in the phit extension, '
                       'phit-polyfill, lme4 lmer() and statsmodels MixedLM.',
        'case': '20,000 rows, 200 groups, 3 predictors',
        'workload': 'Gaussian random intercept by REML, 20,000 rows in 200 groups, 3 predictors',
        'convergence': 'phit and phit-polyfill: BOBYQA over the relative covariance factor, final trust-region '
                       'radius 1e-7 (tolerance), at most 20,000 evaluations, the documented defaults. lme4 lmer(): '
                       'its default nloptwrap, NLopt BOBYQA with xtol_abs 1e-8, ftol_abs 1e-8 and maxeval 1e5, on the '
                       'same profiled REML criterion. statsmodels MixedLM.fit(reml=True): its default method list '
                       'starting with BFGS (gtol 1e-5 on the gradient norm), which optimizes the REML likelihood over '
                       'a different parameterization. The criteria differ, so iteration counts are not comparable, '
                       'and the fixed effects agree to the precision recorded in check.',
    },
}


def dedent(code):
    """The snippet without the indentation its lines after the first have in the script."""
    first, _, rest = code.partition('\n')
    return first + ('\n' + textwrap.dedent(rest) if rest else '')


def command(implementation, workload, path, args):
    script = str(HERE / SCRIPTS[implementation])
    time = [args.time, '-l']
    if implementation == 'phit':
        return time + [args.php, '-n', '-d', 'memory_limit=-1', '-d', f'extension={args.extension}', script, workload, path]
    if implementation == 'phit-polyfill':
        return time + [args.php, '-n', '-d', 'memory_limit=-1', script, workload, path]
    if implementation == 'R':
        return time + ['Rscript', script, workload, path]
    return time + [args.python, script, workload, path]


def run(implementation, workload, path, args):
    env = os.environ | THREADS | {'PHIT_POLYFILL_AUTOLOAD': str(Path(args.polyfill) / 'vendor' / 'autoload.php')}
    out = subprocess.run(command(implementation, workload, path, args), capture_output=True, text=True, env=env)
    if out.returncode != 0:
        raise SystemExit(f'{implementation} {workload} failed:\n{out.stderr}')
    lines = [line for line in out.stdout.splitlines() if line.startswith('{')]
    result = json.loads(lines[-1]) if lines else {}
    # macOS /usr/bin/time -l reports, in bytes, the peak memory footprint (the process's own memory, without clean
    # file pages it maps) and the maximum resident set size (which counts them).
    footprint = re.search(r'(\d+)\s+peak memory footprint', out.stderr)
    rss = re.search(r'(\d+)\s+maximum resident set size', out.stderr)
    result['peak_memory_mib'] = int(footprint.group(1)) / 1048576
    result['peak_rss_mib'] = int(rss.group(1)) / 1048576
    return result


def version(args, workload):
    php = subprocess.run([args.php, '-n', '-r', 'echo PHP_VERSION;'], capture_output=True, text=True).stdout
    r = subprocess.run(['Rscript', '-e', 'cat(paste(R.version$major, R.version$minor, sep = "."), "\\n"); '
                        'cat(as.character(packageVersion("lme4")))'], capture_output=True, text=True).stdout.split()
    py = subprocess.run([args.python, '-c', 'import platform, statsmodels, numpy, pandas; '
                         'print(platform.python_version(), statsmodels.__version__, numpy.__version__, '
                         'pandas.__version__)'], capture_output=True, text=True).stdout.split()
    phit = subprocess.run([args.php, '-n', '-d', f'extension={args.extension}', '-r', 'echo phpversion("phit");'],
                          capture_output=True, text=True).stdout
    polyfill = json.loads((Path(args.polyfill) / 'composer.json').read_text()).get('version') or args.polyfill_version
    versions = {'phit': phit, 'phit-polyfill': polyfill, 'R': r[0], 'Python': py[0], 'statsmodels': py[1],
                'numpy': py[2], 'pandas': py[3]}
    if workload == 'mixed':
        versions['lme4'] = r[1]
    return php, versions


def machine():
    sysctl = lambda key: subprocess.run(['sysctl', '-n', key], capture_output=True, text=True).stdout.strip()
    if sys.platform == 'darwin':
        os_name = f"macOS {platform.mac_ver()[0]} ({platform.machine()})"
        return sysctl('machdep.cpu.brand_string'), int(sysctl('hw.ncpu')), round(int(sysctl('hw.memsize')) / 2**30), os_name
    return platform.processor(), os.cpu_count(), 0, platform.platform()


def benchmark(workload, args):
    path = str(DATA / f'{workload}.csv')
    for implementation in IMPLEMENTATIONS:
        code = CODE[workload]['phit' if implementation == 'phit-polyfill' else implementation]
        if code not in (HERE / SCRIPTS[implementation]).read_text():
            raise SystemExit(f'bench/{SCRIPTS[implementation]} does not contain the recorded code for {implementation}')
    baseline = {i: round(run(i, workload, '--baseline', args)['peak_memory_mib'], 1) for i in IMPLEMENTATIONS}
    for implementation in IMPLEMENTATIONS:
        print(f'{workload}: warm-up {implementation}', file=sys.stderr)
        run(implementation, workload, path, args)
    reps = {i: [] for i in IMPLEMENTATIONS}
    for round_ in range(args.repeats):
        order = IMPLEMENTATIONS[round_ % 4:] + IMPLEMENTATIONS[:round_ % 4]
        for implementation in order:
            reps[implementation].append(run(implementation, workload, path, args))
            print(f'{workload}: round {round_ + 1} {implementation} {reps[implementation][-1]["ms"]:.1f} ms',
                  file=sys.stderr)

    reference = reps['phit'][0]['coefficients']
    difference = max(abs(a - b) for i in IMPLEMENTATIONS for r in reps[i] for a, b in zip(r['coefficients'], reference))
    php, versions = version(args, workload)
    cpu, cores, memory, os_name = machine()
    spec = WORKLOADS[workload]
    commit = subprocess.run(['git', 'rev-parse', '--short', 'HEAD'], capture_output=True, text=True, cwd=ROOT).stdout.strip()
    rows = [{
        'case': spec['case'], 'implementation': i,
        'value': round(statistics.median(r['ms'] for r in reps[i]), 3),
        'reps': [round(r['ms'], 3) for r in reps[i]],
        'peak_memory_mib': round(statistics.median(r['peak_memory_mib'] for r in reps[i]), 1),
        'reps_peak_memory_mib': [round(r['peak_memory_mib'], 1) for r in reps[i]],
        'peak_rss_mib': round(statistics.median(r['peak_rss_mib'] for r in reps[i]), 1),
        'reps_peak_rss_mib': [round(r['peak_rss_mib'], 1) for r in reps[i]],
    } for i in IMPLEMENTATIONS]
    return {
        'title': spec['title'], 'description': spec['description'],
        'measure': 'Elapsed time', 'unit': 'ms', 'statistic': 'median', 'workload': spec['workload'],
        'environment': {
            'cpu': cpu, 'cores': cores, 'memory_gb': memory, 'os': os_name, 'php': php, 'versions': versions,
            'threads': 'One thread everywhere: OPENBLAS_NUM_THREADS, OMP_NUM_THREADS, MKL_NUM_THREADS, '
                       'VECLIB_MAXIMUM_THREADS and BLIS_NUM_THREADS set to 1 for R (OpenBLAS) and numpy. '
                       'phit and phit-polyfill are single-threaded.',
            'repeats': args.repeats, 'date': datetime.now(timezone.utc).date().isoformat(), 'commit': commit,
        },
        'method': 'Each run is a fresh process. Each implementation runs once as a warm-up, then the four alternate '
                  f'for {args.repeats} rounds in a rotated order. The time runs from the data already in memory in '
                  "the runtime's usual structure (PHP arrays of columns, an R data.frame, a pandas DataFrame) to "
                  'holding the coefficients and standard errors, and includes building the model matrix from the '
                  'formula. Reading the CSV file is outside the timing. Every process runs under /usr/bin/time -l. '
                  "Memory is the process's peak memory footprint as macOS reports it, data included: its own "
                  'allocations, without the clean file pages it maps. peak_rss_mib gives the peak resident set '
                  "size, which counts those pages. baseline_memory_mib is each runtime's peak footprint with its "
                  'libraries loaded and no data. ' + spec['convergence'],
        'memory_measure': 'peak memory footprint',
        'check': {'max_abs_coefficient_difference': difference},
        'code': {i: dedent(CODE[workload]['phit' if i == 'phit-polyfill' else i]) for i in IMPLEMENTATIONS},
        'languages': LANGUAGES,
        'baseline_memory_mib': baseline,
        'rows': rows,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--polyfill', required=True, help='phit-polyfill checkout with vendor/ installed')
    parser.add_argument('--polyfill-version', default='', help='recorded when composer.json has no version')
    parser.add_argument('--workload', choices=['ols', 'mixed', 'all'], default='all')
    parser.add_argument('--repeats', type=int, default=7)
    parser.add_argument('--php', default=subprocess.run(['php-config', '--php-binary'], capture_output=True,
                                                        text=True).stdout.strip())
    parser.add_argument('--extension', default=str(ROOT / 'target' / 'release' / 'libphit.dylib'))
    parser.add_argument('--python', default=str(ROOT / '.context' / 'bench-venv' / 'bin' / 'python'))
    parser.add_argument('--time', default='/usr/bin/time')
    args = parser.parse_args()
    for workload in ['ols', 'mixed'] if args.workload == 'all' else [args.workload]:
        record = benchmark(workload, args)
        out = ROOT / 'site' / 'benchmarks' / f'{workload}-workload.json'
        out.write_text(json.dumps(record, indent=1) + '\n')
        print(f'wrote {out.relative_to(ROOT)}', file=sys.stderr)


if __name__ == '__main__':
    main()
