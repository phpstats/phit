"""Runs the workload benchmarks and writes site/benchmarks/{workload}-workload.json.

    python bench/run.py --polyfill /path/to/phit-polyfill [--workload NAME|all] [--repeats 7]

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
LANGUAGES = {'phit': 'php', 'phit-polyfill': 'php', 'R': 'r', 'statsmodels': 'python', 'GPBoost': 'python',
             'SciPy': 'python'}
SCRIPTS = {'php': 'fit.php', 'r': 'fit.R', 'python': 'fit.py'}
# How memory is measured: /usr/bin/time -l on macOS, and bench/peakmem.c, which reports the same two lines, on Linux.
MEMORY_METHOD = {
    True: "Every process runs under /usr/bin/time -l. Memory is the process's peak memory footprint as macOS "
          "reports it, data included: its own allocations, without the clean file pages it maps. The peak resident "
          "set size, which counts those pages, and each runtime's baseline memory with its libraries loaded and no "
          "data are recorded beside it. ",
    False: "Every process runs under bench/peakmem.c. Memory is the process's peak anonymous memory, data "
           "included: its own allocations, without the file pages it maps, from RssAnon in /proc, sampled every "
           "millisecond. The peak resident set size, which counts those pages, and each runtime's baseline memory "
           "with its libraries loaded and no data are recorded beside it. ",
}

WORKLOADS = {
    'ols': {
        'implementations': ['phit', 'phit-polyfill', 'R', 'statsmodels'],
        'data': 'ols.csv',
        'title': 'OLS, 1M rows',
        'label': 'OLS, 1M rows',
        'description': 'Least-squares fit of a linear model from a formula, returning coefficients and standard '
                       'errors. Implementations: the phit extension, phit-polyfill, R lm(), and statsmodels OLS.',
        'case': '1,000,000 rows, 10 predictors',
        'workload': 'OLS, 1,000,000 rows, 10 predictors',
        'convergence': 'OLS has no iterations. phit and phit-polyfill solve by column-scaled SVD. R lm() uses a '
                       'Householder QR (LINPACK dqrls). statsmodels OLS.fit() uses its default method, "pinv": '
                       'the Moore-Penrose pseudoinverse by SVD.',
    },
    'mixed': {
        'implementations': ['phit', 'phit-polyfill', 'R', 'statsmodels'],
        'data': 'mixed.csv',
        'title': 'Linear mixed model, 20k rows',
        'label': 'Linear mixed model, 20k rows',
        'description': 'REML fit of a Gaussian random-intercept model from a formula, returning fixed-effect '
                       'coefficients and standard errors. Implementations: the phit extension, phit-polyfill, lme4 '
                       'lmer(), and statsmodels MixedLM.',
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
    'logistic': {
        'implementations': ['phit', 'phit-polyfill', 'R', 'statsmodels'],
        'data': 'binary.csv',
        'title': 'Logistic regression, 1M rows',
        'label': 'Logistic regression, 1M rows',
        'description': 'Maximum-likelihood fit of a binomial logit model from a formula, returning coefficients and '
                       'standard errors. Implementations: the phit extension, phit-polyfill, R glm(), and '
                       'statsmodels Logit.',
        'case': '1,000,000 rows, 10 predictors',
        'workload': 'Logistic regression, 1,000,000 rows, 10 predictors, 0/1 response',
        'convergence': 'phit and phit-polyfill: Newton/IRLS solved by column-scaled weighted SVD, starting at zero, '
                       'until each scaled coefficient step is at most 1e-8 x (1 + |coefficient|) and the '
                       'log-likelihood change at most 1e-10 x (1 + |log-likelihood|). R glm(): IRLS until the '
                       'relative deviance change is below epsilon 1e-8, at most 25 iterations. statsmodels '
                       'Logit.fit(method="newton"): Newton-Raphson until the parameter change is below tol 1e-8, at '
                       'most 35 iterations.',
    },
    'probit': {
        'implementations': ['phit', 'phit-polyfill', 'R', 'statsmodels'],
        'data': 'binary.csv',
        'title': 'Probit regression, 1M rows',
        'label': 'Probit regression, 1M rows',
        'description': "Maximum-likelihood fit of a binomial probit model from a formula on the logistic workload's "
                       'data, returning coefficients and standard errors. Implementations: the phit extension, '
                       'phit-polyfill, R glm(), and statsmodels Probit.',
        'case': '1,000,000 rows, 10 predictors',
        'workload': 'Probit regression, 1,000,000 rows, 10 predictors, 0/1 response',
        'convergence': 'phit and phit-polyfill: Fisher scoring solved by column-scaled weighted SVD, until each '
                       'scaled coefficient step is at most 1e-11 x (1 + |coefficient|) and the log-likelihood change '
                       'at most 1e-13 x (1 + |log-likelihood|), at most 100 updates. R glm(binomial("probit")): IRLS '
                       'until the relative deviance change is below epsilon 1e-8, at most 25 iterations. statsmodels '
                       'Probit.fit(method="newton"): Newton-Raphson until the parameter change is below tol 1e-8, at '
                       'most 35 iterations.',
    },
    'mixed-logistic': {
        'implementations': ['phit', 'phit-polyfill', 'R', 'GPBoost'],
        'data': 'mixed-binary.csv',
        'title': 'Binomial mixed model, 20k rows',
        'label': 'Binomial mixed model, 20k rows',
        'description': 'Laplace fit of a binomial logit model with one random intercept, returning fixed-effect '
                       'coefficients and standard errors. Implementations: the phit extension, phit-polyfill, lme4 '
                       'glmer(), and GPBoost.',
        'case': '20,000 rows, 200 groups, 3 predictors',
        'workload': 'Binomial logit random intercept by Laplace, 20,000 rows in 200 groups, 3 predictors, 0/1 '
                    'response',
        'convergence': 'phit and phit-polyfill: BOBYQA over the fixed effects and the group SD, starting from the '
                       'binomial GLM, final trust-region radius 1e-7, at most 20,000 evaluations, with each group\'s '
                       'conditional mode solved by bracketed Newton steps. lme4 glmer(): nAGQ = 1 with its default '
                       'two stages (bobyqa, then Nelder_Mead) and glmerControl(tolPwrss = 1e-10), tighter than the '
                       'default 1e-7, which stops the conditional-mode iterations early enough to move the estimates '
                       '(docs/r-compatibility.qmd). GPBoost 1.8.0: GPModel(likelihood="bernoulli_logit") with '
                       'optimizer_cov "lbfgs" and delta_rel_conv 1e-12, single-threaded through OMP_NUM_THREADS.',
    },
    'distributions': {
        'implementations': ['phit', 'phit-polyfill', 'R', 'SciPy'],
        'data': None,
        'title': 'Distributions, 1M inputs',
        'label': 'Distributions, 1M inputs',
        'description': 'Normal and gamma CDF and quantile functions over 1,000,000 inputs each, by batch calls. '
                       'Implementations: the phit extension, phit-polyfill, R, and SciPy.',
        'case': '4 functions x 1,000,000 inputs',
        'workload': 'Normal CDF and quantile, gamma (shape 2.5, scale 1.5) CDF and quantile, 1,000,000 inputs each',
        'convergence': 'No iterations to compare. phit and phit-polyfill use the _array batch functions (cdflib '
                       'normal and incomplete-gamma backends). R uses its vectorized pnorm, qnorm, pgamma and qgamma. '
                       'SciPy uses stats.norm and stats.gamma cdf and ppf on numpy arrays.',
    },
}


def mbstring(php):
    """The -d option that loads mbstring, when PHP without php.ini lacks it."""
    built_in = subprocess.run([php, '-n', '-r', 'echo extension_loaded("mbstring") ? 1 : 0;'], capture_output=True,
                              text=True).stdout.strip() == '1'
    return [] if built_in else ['-d', 'extension=mbstring']


def command(implementation, workload, path, args):
    script = str(HERE / SCRIPTS[LANGUAGES[implementation]])
    time = [args.time, '-l']
    # -n leaves out php.ini. Where mbstring is a shared extension, as on Ubuntu, the polyfill needs it loaded
    # explicitly; both PHP implementations get it, so they run with the same extensions.
    php = [args.php, '-n', '-d', 'memory_limit=-1', *mbstring(args.php)]
    if implementation == 'phit':
        return time + php + ['-d', f'extension={args.extension}', script, workload, path]
    if implementation == 'phit-polyfill':
        return time + php + [script, workload, path]
    if implementation == 'R':
        return time + ['Rscript', script, workload, path]
    return time + [args.python, script, workload, path]


def timed_code(implementation, workload):
    """The lines between the script's [workload] and [/workload] markers, dedented."""
    text = (HERE / SCRIPTS[LANGUAGES[implementation]]).read_text()
    match = re.search(rf'^[ \t]*(?://|#) \[{re.escape(workload)}\]\n(.*?)^[ \t]*(?://|#) \[/{re.escape(workload)}\]',
                      text, re.S | re.M)
    if not match:
        raise SystemExit(f'{SCRIPTS[LANGUAGES[implementation]]} has no [{workload}] block')
    return textwrap.dedent(match.group(1)).rstrip('\n')


def run(implementation, workload, path, args):
    env = os.environ | THREADS | {'PHIT_POLYFILL_AUTOLOAD': str(Path(args.polyfill) / 'vendor' / 'autoload.php')}
    out = subprocess.run(command(implementation, workload, path, args), capture_output=True, text=True, env=env)
    if out.returncode != 0:
        raise SystemExit(f'{implementation} {workload} failed:\n{out.stderr}')
    lines = [line for line in out.stdout.splitlines() if line.startswith('{')]
    result = json.loads(lines[-1]) if lines else {}
    # macOS /usr/bin/time -l reports, in bytes, the peak memory footprint (the process's own memory, without clean
    # file pages it maps) and the maximum resident set size (which counts them). On Linux, bench/peakmem.c reports
    # the same two lines, with the peak of the process's anonymous resident memory as its footprint.
    footprint = re.search(r'(\d+)\s+peak memory footprint', out.stderr)
    rss = re.search(r'(\d+)\s+maximum resident set size', out.stderr)
    result['peak_memory_mib'] = int(footprint.group(1)) / 1048576
    result['peak_rss_mib'] = int(rss.group(1)) / 1048576
    return result


def version(args, workload):
    implementations = WORKLOADS[workload]['implementations']
    php = subprocess.run([args.php, '-n', '-r', 'echo PHP_VERSION;'], capture_output=True, text=True).stdout
    r = subprocess.run(['Rscript', '-e', 'cat(paste(R.version$major, R.version$minor, sep = "."), "\\n"); '
                        'cat(as.character(packageVersion("lme4")))'], capture_output=True, text=True).stdout.split()
    py = json.loads(subprocess.run([args.python, '-c', 'import json, platform, importlib.metadata as m; print(json.dumps('
                                    '{"Python": platform.python_version(), **{k: m.version(k) for k in '
                                    '["statsmodels", "numpy", "pandas", "scipy", "gpboost"]}}))'],
                                   capture_output=True, text=True).stdout)
    phit = subprocess.run([args.php, '-n', '-d', f'extension={args.extension}', '-r', 'echo phpversion("phit");'],
                          capture_output=True, text=True).stdout
    polyfill = json.loads((Path(args.polyfill) / 'composer.json').read_text()).get('version') or args.polyfill_version
    versions = {'phit': phit, 'phit-polyfill': polyfill, 'R': r[0], 'Python': py['Python'], 'numpy': py['numpy']}
    if workload in ('mixed', 'mixed-logistic'):
        versions['lme4'] = r[1]
    if 'statsmodels' in implementations:
        versions |= {'statsmodels': py['statsmodels'], 'pandas': py['pandas']}
    if 'GPBoost' in implementations:
        versions |= {'gpboost': py['gpboost'], 'pandas': py['pandas']}
    if 'SciPy' in implementations:
        versions['scipy'] = py['scipy']
    return php, versions


def machine():
    sysctl = lambda key: subprocess.run(['sysctl', '-n', key], capture_output=True, text=True).stdout.strip()
    if sys.platform == 'darwin':
        os_name = f"macOS {platform.mac_ver()[0]} ({platform.machine()})"
        return sysctl('machdep.cpu.brand_string'), int(sysctl('hw.ncpu')), round(int(sysctl('hw.memsize')) / 2**30), os_name
    cpu = re.search(r'^model name\s*:\s*(.+)$', Path('/proc/cpuinfo').read_text(), re.M).group(1).strip()
    kib = int(re.search(r'^MemTotal:\s*(\d+)', Path('/proc/meminfo').read_text(), re.M).group(1))
    release = re.search(r'^PRETTY_NAME="(.+)"', Path('/etc/os-release').read_text(), re.M).group(1)
    return cpu, os.cpu_count(), round(kib / 2**20), f'Linux, {release} ({platform.machine()})'


def record_text(workload, implementations, repeats, darwin):
    """A record's reader-facing text: title, label, Scope, workload, method, and memory measure."""
    spec = WORKLOADS[workload]
    inputs = ('The time runs from the inputs already in memory (a PHP array, an R vector, a numpy array) to '
              'holding all four results. Building the inputs is outside the timing. '
              if workload == 'distributions' else
              "The time runs from the data already in memory in the runtime's usual structure (PHP arrays of "
              'columns, an R data.frame, a pandas DataFrame, or numpy arrays for GPBoost) to holding the '
              'coefficients and standard errors, and includes building the model matrix from the formula. '
              'Reading the CSV file is outside the timing. ')
    return {
        'title': spec['title'], 'label': spec['label'], 'description': spec['description'],
        'workload': spec['workload'],
        'method': f'Each run is a fresh process. Each implementation runs once as a warm-up, then the '
                  f'{implementations} alternate for {repeats} rounds in a rotated order. ' + inputs
                  + MEMORY_METHOD[darwin] + spec['convergence'],
        'memory_measure': 'peak memory footprint' if darwin else 'peak anonymous memory',
    }


def benchmark(workload, args):
    spec = WORKLOADS[workload]
    implementations = spec['implementations']
    path = str(DATA / spec['data']) if spec['data'] else '-'
    code = {i: timed_code(i, workload) for i in implementations}
    baseline = {i: round(run(i, workload, '--baseline', args)['peak_memory_mib'], 1) for i in implementations}
    for implementation in implementations:
        print(f'{workload}: warm-up {implementation}', file=sys.stderr)
        run(implementation, workload, path, args)
    reps = {i: [] for i in implementations}
    for round_ in range(args.repeats):
        shift = round_ % len(implementations)
        order = implementations[shift:] + implementations[:shift]
        for implementation in order:
            reps[implementation].append(run(implementation, workload, path, args))
            print(f'{workload}: round {round_ + 1} {implementation} {reps[implementation][-1]["ms"]:.1f} ms',
                  file=sys.stderr)

    if workload == 'distributions':
        # Every 1000th output of each function; the largest relative difference from SciPy.
        reference = reps['SciPy'][0]['values']
        check = {'max_relative_difference_from_scipy': max(
            abs(a - b) / max(abs(b), 1e-300) for i in implementations for r in reps[i]
            for a, b in zip(r['values'], reference)), 'compared_values': len(reference)}
    else:
        reference = reps['phit'][0]['coefficients']
        check = {'max_abs_coefficient_difference': max(
            abs(a - b) for i in implementations for r in reps[i] for a, b in zip(r['coefficients'], reference))}
    php, versions = version(args, workload)
    cpu, cores, memory, os_name = machine()
    commit = subprocess.run(['git', 'rev-parse', '--short', 'HEAD'], capture_output=True, text=True, cwd=ROOT).stdout.strip()
    rows = [{
        'case': spec['case'], 'implementation': i,
        'value': round(statistics.median(r['ms'] for r in reps[i]), 3),
        'reps': [round(r['ms'], 3) for r in reps[i]],
        'peak_memory_mib': round(statistics.median(r['peak_memory_mib'] for r in reps[i]), 1),
        'reps_peak_memory_mib': [round(r['peak_memory_mib'], 1) for r in reps[i]],
        'peak_rss_mib': round(statistics.median(r['peak_rss_mib'] for r in reps[i]), 1),
        'reps_peak_rss_mib': [round(r['peak_rss_mib'], 1) for r in reps[i]],
    } for i in implementations]
    return {
        'measure': 'Elapsed time', 'unit': 'ms', 'statistic': 'median',
        'environment': {
            'cpu': cpu, 'cores': cores, 'memory_gb': memory, 'os': os_name, 'php': php, 'versions': versions,
            'threads': 'One thread everywhere: OPENBLAS_NUM_THREADS, OMP_NUM_THREADS, MKL_NUM_THREADS, '
                       'VECLIB_MAXIMUM_THREADS and BLIS_NUM_THREADS set to 1 for R (OpenBLAS), numpy and GPBoost '
                       '(OpenMP). '
                       'phit and phit-polyfill are single-threaded.',
            'repeats': args.repeats, 'date': datetime.now(timezone.utc).date().isoformat(), 'commit': commit,
            'code_commit': commit,
        },
        **record_text(workload, len(implementations), args.repeats, sys.platform == 'darwin'),
        'check': check,
        'code': code,
        'languages': {i: LANGUAGES[i] for i in implementations},
        'baseline_memory_mib': baseline,
        'rows': rows,
    }


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--polyfill', help='phit-polyfill checkout with vendor/ installed')
    parser.add_argument('--polyfill-version', default='', help='recorded when composer.json has no version')
    parser.add_argument('--workload', choices=[*WORKLOADS, 'all'], default='all')
    parser.add_argument('--repeats', type=int, default=7)
    parser.add_argument('--php', default=subprocess.run(['php-config', '--php-binary'], capture_output=True,
                                                        text=True).stdout.strip())
    parser.add_argument('--extension', default=str(ROOT / 'target' / 'release' / (
        'libphit.dylib' if sys.platform == 'darwin' else 'libphit.so')))
    parser.add_argument('--python', default=str(ROOT / '.context' / 'bench-venv' / 'bin' / 'python'))
    # On Linux, build bench/peakmem.c and put it on the PATH: cc -O2 -o peakmem bench/peakmem.c
    parser.add_argument('--time', default='/usr/bin/time' if sys.platform == 'darwin' else 'peakmem')
    parser.add_argument('--text-only', action='store_true',
                        help="rewrite only each existing record's title, label, Scope, workload, and method")
    parser.add_argument('--code-only', action='store_true',
                        help="rewrite only each existing record's code from the scripts, with code_commit set to HEAD")
    args = parser.parse_args()
    if args.text_only:
        for workload in list(WORKLOADS) if args.workload == 'all' else [args.workload]:
            out = ROOT / 'site' / 'benchmarks' / f'{workload}-workload.json'
            record = json.loads(out.read_text())
            text = record_text(workload, len(record['code']), record['environment']['repeats'],
                               record['memory_measure'] == 'peak memory footprint')
            record = {**{k: text[k] for k in ('title', 'label')}, **{k: v for k, v in record.items() if k not in text},
                      **{k: text[k] for k in text if k not in ('title', 'label')}}
            out.write_text(json.dumps(record, indent=1) + '\n')
            print(f'rewrote the text of {out.relative_to(ROOT)}', file=sys.stderr)
        return
    if args.code_only:
        commit = subprocess.run(['git', 'rev-parse', '--short', 'HEAD'], capture_output=True, text=True,
                                cwd=ROOT).stdout.strip()
        for workload in list(WORKLOADS) if args.workload == 'all' else [args.workload]:
            out = ROOT / 'site' / 'benchmarks' / f'{workload}-workload.json'
            record = json.loads(out.read_text())
            record['code'] = {i: timed_code(i, workload) for i in record['code']}
            record['environment']['code_commit'] = commit
            out.write_text(json.dumps(record, indent=1) + '\n')
            print(f'rewrote the code of {out.relative_to(ROOT)}', file=sys.stderr)
        return
    if not args.polyfill:
        parser.error('--polyfill is required to run the benchmarks')
    for workload in list(WORKLOADS) if args.workload == 'all' else [args.workload]:
        record = benchmark(workload, args)
        out = ROOT / 'site' / 'benchmarks' / f'{workload}-workload.json'
        out.write_text(json.dumps(record, indent=1) + '\n')
        print(f'wrote {out.relative_to(ROOT)}', file=sys.stderr)


if __name__ == '__main__':
    main()
