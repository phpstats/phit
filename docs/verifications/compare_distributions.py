"""Independent references for the distribution API: SciPy values and mpmath.

Inputs: tests/fixtures/distributions-r.json (cases and R values) and the PHP
extension, run through docs/verifications/distributions.php.

mpmath evaluates closed-form densities and regularized incomplete beta/gamma
integrals at 60 significant digits with an unbounded exponent range, so tails
far below the double range remain exact. Only the smaller tail is computed by
summation (a positive-term series, or Lentz's continued fraction when the
series converges slowly); the larger tail is 1 minus it and its log is
log1p(-small). Continuous reference quantiles are Newton iterations on the
smaller tail, carried out in mpmath from the PHP value (or R's); discrete
reference quantiles satisfy F(k - 1) < p <= F(k) exactly. Every reference is
rounded once to the nearest double.

Outputs:
  tests/fixtures/distributions-scipy.json   SciPy values for every case
  tests/fixtures/distributions-mpmath.json  correctly rounded references (null if unavailable)
  docs/verifications/distributions-results.json  errors of PHP, R and SciPy vs mpmath

Usage: python3 docs/verifications/compare_distributions.py [--php-results FILE]
"""
import argparse
import hashlib
import json
import math
import os
import platform
import signal
import subprocess
import sys
from datetime import datetime, timezone
from pathlib import Path

import mpmath as mp
import numpy as np
import scipy
from scipy import stats

ROOT = Path(__file__).resolve().parents[2]
mp.mp.dps = 60
TIMEOUT_SECONDS = 60
EPS = mp.mpf(10) ** (-mp.mp.dps + 5)
DISCRETE = ('binomial', 'poisson', 'nbinom', 'nbinom_mu')


def number(v):
    if isinstance(v, str):
        return {'Inf': math.inf, '-Inf': -math.inf, 'NaN': math.nan}[v]
    return float(v)


def encode(v):
    if v is None:
        return None
    v = float(v)
    if math.isnan(v):
        return 'NaN'
    if math.isinf(v):
        return 'Inf' if v > 0 else '-Inf'
    return v


class Timeout(Exception):
    pass


def _alarm(signum, frame):
    raise Timeout()


signal.signal(signal.SIGALRM, _alarm)


def bounded(f, *args):
    signal.alarm(TIMEOUT_SECONDS)
    try:
        return f(*args)
    except Timeout:
        return None
    finally:
        signal.alarm(0)


# ---- incomplete beta and gamma in mpmath ---------------------------------------
def beta_front(a, b, x):
    """x^a (1 - x)^b / (a B(a, b))."""
    return mp.exp(a * mp.log(x) + b * mp.log1p(-x) - mp.log(a)
                  - (mp.loggamma(a) + mp.loggamma(b) - mp.loggamma(a + b)))


def beta_small(a, b, x):
    """I_x(a, b) for x < (a + 1)/(a + b + 2), where it is the smaller tail."""
    ratio = (a + b) / (a + 1) * x
    if ratio < mp.mpf('0.95'):
        # Positive-term hypergeometric series: no cancellation.
        term, total, n = mp.mpf(1), mp.mpf(1), 0
        while True:
            term *= (a + b + n) / (a + 1 + n) * x
            total += term
            n += 1
            if term < total * EPS:
                return beta_front(a, b, x) * total
    # Lentz's continued fraction (DLMF 8.17.22), converging in O(sqrt(a + b)) terms.
    tiny = mp.mpf(10) ** -400
    c, d = mp.mpf(1), 1 - (a + b) * x / (a + 1)
    d = 1 / (d if abs(d) > tiny else tiny)
    h, m = d, 0
    while True:
        m += 1
        for numerator in (m * (b - m) * x / ((a + 2 * m - 1) * (a + 2 * m)),
                          -(a + m) * (a + b + m) * x / ((a + 2 * m) * (a + 2 * m + 1))):
            d = 1 + numerator * d
            d = 1 / (d if abs(d) > tiny else tiny)
            c = 1 + numerator / c
            c = c if abs(c) > tiny else tiny
            delta = c * d
            h *= delta
        if abs(delta - 1) < EPS:
            return beta_front(a, b, x) * h


def beta_tails(a, b, x):
    """(I_x(a, b), 1 - I_x(a, b)) with the smaller one summed directly."""
    if x <= 0:
        return mp.mpf(0), mp.mpf(1)
    if x >= 1:
        return mp.mpf(1), mp.mpf(0)
    if x < (a + 1) / (a + b + 2):
        lower = beta_small(a, b, x)
        return lower, 1 - lower
    upper = beta_small(b, a, 1 - x)
    return 1 - upper, upper


def gamma_tails(a, x):
    """(P(a, x), Q(a, x)) regularized; mpmath computes each tail directly."""
    if x <= 0:
        return mp.mpf(0), mp.mpf(1)
    if mp.isinf(x):
        return mp.mpf(1), mp.mpf(0)
    if x < a + 1:
        lower = mp.gammainc(a, 0, x, regularized=True)
        return lower, 1 - lower
    upper = mp.gammainc(a, x, mp.inf, regularized=True)
    return 1 - upper, upper


class Reference:
    def __init__(self, name, params):
        self.name = name
        self.p = {k: mp.mpf(v) for k, v in params.items()}
        self.discrete = name in DISCRETE
        if name in ('nbinom', 'nbinom_mu'):
            r = self.p['size']
            if name == 'nbinom':
                self.pr, self.q = self.p['prob'], 1 - self.p['prob']
            else:
                self.pr, self.q = r / (r + self.p['mu']), self.p['mu'] / (r + self.p['mu'])

    def support(self):
        if self.name in ('normal', 't', 'logistic'):
            return -mp.inf, mp.inf
        if self.name == 'beta':
            return mp.mpf(0), mp.mpf(1)
        return mp.mpf(0), mp.inf

    def logdensity(self, x):
        p, n = self.p, self.name
        if self.discrete and (mp.isinf(x) or x < 0 or x != mp.floor(x)):
            return -mp.inf
        if n == 'normal':
            if mp.isinf(x):
                return -mp.inf
            z = (x - p['mean']) / p['sd']
            return -z * z / 2 - mp.log(p['sd']) - mp.log(2 * mp.pi) / 2
        if n == 'lognormal':
            if x <= 0 or mp.isinf(x):
                return -mp.inf
            z = (mp.log(x) - p['meanlog']) / p['sdlog']
            return -z * z / 2 - mp.log(p['sdlog'] * x) - mp.log(2 * mp.pi) / 2
        if n == 'logistic':
            if mp.isinf(x):
                return -mp.inf
            z = abs((x - p['location']) / p['scale'])
            return -z - 2 * mp.log1p(mp.exp(-z)) - mp.log(p['scale'])
        if n == 'exponential':
            return -mp.inf if x < 0 or mp.isinf(x) else mp.log(p['rate']) - p['rate'] * x
        if n == 't':
            if mp.isinf(x):
                return -mp.inf
            v = p['df']
            return (mp.loggamma((v + 1) / 2) - mp.loggamma(v / 2) - mp.log(mp.pi * v) / 2
                    - (v + 1) / 2 * mp.log1p(x * x / v))
        if n in ('chisquare', 'gamma'):
            a, s = (p['df'] / 2, mp.mpf(2)) if n == 'chisquare' else (p['shape'], p['scale'])
            if x < 0 or mp.isinf(x):
                return -mp.inf
            if x == 0:
                return mp.inf if a < 1 else (-mp.log(s) if a == 1 else -mp.inf)
            return (a - 1) * mp.log(x) - x / s - mp.loggamma(a) - a * mp.log(s)
        if n == 'beta':
            a, b = p['alpha'], p['beta']
            if x < 0 or x > 1:
                return -mp.inf
            if x == 0:
                return mp.inf if a < 1 else (mp.log(b) if a == 1 else -mp.inf)
            if x == 1:
                return mp.inf if b < 1 else (mp.log(a) if b == 1 else -mp.inf)
            return ((a - 1) * mp.log(x) + (b - 1) * mp.log1p(-x)
                    - (mp.loggamma(a) + mp.loggamma(b) - mp.loggamma(a + b)))
        if n == 'f':
            m, d = p['df1'], p['df2']
            if x < 0 or mp.isinf(x):
                return -mp.inf
            if x == 0:
                return mp.inf if m < 2 else (mp.mpf(0) if m == 2 else -mp.inf)
            return (m / 2 * mp.log(m / d) + (m / 2 - 1) * mp.log(x) - (m + d) / 2 * mp.log1p(m * x / d)
                    - (mp.loggamma(m / 2) + mp.loggamma(d / 2) - mp.loggamma((m + d) / 2)))
        if n == 'binomial':
            size, pr = p['size'], p['prob']
            if x > size:
                return -mp.inf
            if pr == 0:
                return mp.mpf(0) if x == 0 else -mp.inf
            if pr == 1:
                return mp.mpf(0) if x == size else -mp.inf
            return (mp.loggamma(size + 1) - mp.loggamma(x + 1) - mp.loggamma(size - x + 1)
                    + x * mp.log(pr) + (size - x) * mp.log1p(-pr))
        if n == 'poisson':
            lam = p['lambda']
            if lam == 0:
                return mp.mpf(0) if x == 0 else -mp.inf
            return x * mp.log(lam) - lam - mp.loggamma(x + 1)
        r = p['size']
        if self.q == 0:
            return mp.mpf(0) if x == 0 else -mp.inf
        return (mp.loggamma(x + r) - mp.loggamma(r) - mp.loggamma(x + 1)
                + r * mp.log(self.pr) + x * mp.log(self.q))

    def tails(self, x):
        """(P(X <= x), P(X > x))."""
        p, n = self.p, self.name
        if n in ('normal', 'lognormal', 'logistic', 't') and mp.isinf(x):
            return (mp.mpf(1), mp.mpf(0)) if x > 0 else (mp.mpf(0), mp.mpf(1))
        if n in ('normal', 'lognormal'):
            if n == 'lognormal':
                if x <= 0:
                    return mp.mpf(0), mp.mpf(1)
                z = (mp.log(x) - p['meanlog']) / p['sdlog']
            else:
                z = (x - p['mean']) / p['sd']
            return mp.erfc(-z / mp.sqrt(2)) / 2, mp.erfc(z / mp.sqrt(2)) / 2
        if n == 'logistic':
            z = (x - p['location']) / p['scale']
            return 1 / (1 + mp.exp(-z)), 1 / (1 + mp.exp(z))
        if n == 'exponential':
            if x <= 0:
                return mp.mpf(0), mp.mpf(1)
            return -mp.expm1(-p['rate'] * x), mp.exp(-p['rate'] * x)
        if n == 't':
            v = p['df']
            small = beta_tails(v / 2, mp.mpf(1) / 2, v / (v + x * x))[0] / 2
            return (small, 1 - small) if x <= 0 else (1 - small, small)
        if n in ('chisquare', 'gamma'):
            a, s = (p['df'] / 2, mp.mpf(2)) if n == 'chisquare' else (p['shape'], p['scale'])
            return gamma_tails(a, x / s)
        if n == 'beta':
            return beta_tails(p['alpha'], p['beta'], x)
        if n == 'f':
            m, d = p['df1'], p['df2']
            if x <= 0:
                return mp.mpf(0), mp.mpf(1)
            if mp.isinf(x):
                return mp.mpf(1), mp.mpf(0)
            return beta_tails(m / 2, d / 2, m * x / (m * x + d))
        if x < 0:
            return mp.mpf(0), mp.mpf(1)
        if mp.isinf(x):
            return mp.mpf(1), mp.mpf(0)
        k = mp.floor(x)
        if n == 'binomial':
            size, pr = p['size'], p['prob']
            if k >= size or pr == 0:
                return mp.mpf(1), mp.mpf(0)
            if pr == 1:
                return mp.mpf(0), mp.mpf(1)
            upper, lower = beta_tails(k + 1, size - k, pr)
            return lower, upper
        if n == 'poisson':
            if p['lambda'] == 0:
                return mp.mpf(1), mp.mpf(0)
            upper, lower = gamma_tails(k + 1, p['lambda'])
            return lower, upper
        if self.q == 0:
            return mp.mpf(1), mp.mpf(0)
        return beta_tails(p['size'], k + 1, self.pr)

    def value(self, op, x):
        if op in ('pdf', 'pmf'):
            return mp.exp(self.logdensity(x))
        if op in ('logpdf', 'logpmf'):
            return self.logdensity(x)
        lower, upper = self.tails(x)
        if op == 'cdf':
            return lower
        if op == 'sf':
            return upper
        if op == 'logcdf':
            return mp.log(lower) if lower <= upper else mp.log1p(-upper)
        return mp.log(upper) if upper <= lower else mp.log1p(-lower)

    def quantile(self, p, starts):
        """Reference quantile for probability p (a double, used exactly)."""
        p = mp.mpf(p)
        lo, hi = self.support()
        if self.discrete:
            if p == 0:
                return mp.mpf(0)
            # Degenerate count distributions have a finite atom even at p=1.
            # Check this before the generic upper-support endpoint convention.
            if self.name == 'binomial' and self.p['prob'] == 0:
                return mp.mpf(0)
            if self.name == 'binomial' and self.p['prob'] == 1:
                return self.p['size']
            if self.name == 'poisson' and self.p['lambda'] == 0:
                return mp.mpf(0)
            if self.name in ('nbinom', 'nbinom_mu') and self.pr == 1:
                return mp.mpf(0)
            if p == 1:
                return hi if self.name != 'binomial' else self.p['size']
            def reached(k):
                lower, upper = self.tails(k)
                return lower >= p if p <= mp.mpf(0.5) else upper <= 1 - p
            finite = [s for s in starts if s is not None and math.isfinite(s)]
            if not finite:
                return mp.inf if not reached(mp.mpf(2) ** 70) else None
            k = mp.mpf(max(0, math.floor(finite[0])))
            for _ in range(10000):
                if reached(k):
                    if k == 0 or not reached(k - 1):
                        return k
                    k -= 1
                else:
                    k += 1
            return None
        if p == 0:
            return lo
        if p == 1:
            return hi
        use_lower = p <= mp.mpf(0.5)
        target = p if use_lower else 1 - p
        for start in starts:
            if start is None or not math.isfinite(start) or (start == 0 and lo == 0):
                continue
            x = mp.mpf(start)
            for _ in range(60):
                lower, upper = self.tails(x)
                tail = lower if use_lower else upper
                density = mp.exp(self.logdensity(x))
                if density == 0 or tail == 0:
                    break
                # Newton on the log tail: d ln F/dx = f/F, d ln S/dx = -f/S.
                step = (mp.log(tail) - mp.log(target)) * tail / density
                x = x - step if use_lower else x + step
                if lo == 0 and x <= 0:
                    break
                if abs(step) <= abs(x) * EPS:
                    lower, upper = self.tails(x)
                    tail = lower if use_lower else upper
                    if abs(tail / target - 1) < mp.mpf(10) ** -30:
                        return x
                    break
        return None


# ---- SciPy -----------------------------------------------------------------------
def scipy_distribution(name, p):
    return {
        'normal': lambda: stats.norm(loc=p['mean'], scale=p['sd']),
        't': lambda: stats.t(p['df']),
        'chisquare': lambda: stats.chi2(p['df']),
        'f': lambda: stats.f(p['df1'], p['df2']),
        'beta': lambda: stats.beta(p['alpha'], p['beta']),
        'gamma': lambda: stats.gamma(p['shape'], scale=p['scale']),
        'exponential': lambda: stats.expon(scale=1 / p['rate']),
        'lognormal': lambda: stats.lognorm(p['sdlog'], scale=math.exp(p['meanlog'])),
        'logistic': lambda: stats.logistic(loc=p['location'], scale=p['scale']),
        'binomial': lambda: stats.binom(int(p['size']), p['prob']),
        'poisson': lambda: stats.poisson(p['lambda']),
        'nbinom': lambda: stats.nbinom(p['size'], p['prob']),
        'nbinom_mu': lambda: stats.nbinom(p['size'], p['size'] / (p['size'] + p['mu'])),
    }[name]()


SCIPY_METHOD = {'pdf': 'pdf', 'pmf': 'pmf', 'logpdf': 'logpdf', 'logpmf': 'logpmf', 'cdf': 'cdf',
                'logcdf': 'logcdf', 'sf': 'sf', 'logsf': 'logsf', 'quantile': 'ppf'}


def round_reference(reference):
    """Nearest double to an mpf (±inf beyond the double range)."""
    if reference is None:
        return None
    try:
        return float(reference)  # mpmath rounds to nearest
    except OverflowError:
        return math.inf if reference > 0 else -math.inf


def error(actual, reference, op):
    """Relative error against the rounded reference (absolute below 1 for logs
    and quantiles near 0); exact agreement needed for nonfinite references."""
    if reference is None:
        return None
    if isinstance(actual, dict):
        return math.inf
    if actual == reference:
        return 0.0
    if math.isnan(actual) or math.isinf(actual) or math.isinf(reference):
        return math.inf
    scale = abs(reference)
    if op.startswith('log') or op == 'quantile':
        scale = max(scale, 1.0) if op.startswith('log') else max(scale, 1e-300)
    elif reference == 0:
        return math.inf
    elif scale < sys.float_info.min:
        scale = sys.float_info.min  # subnormal results have reduced precision
    return abs(actual - reference) / scale


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument('--php-results', type=Path)
    parser.add_argument('--stored-references', action='store_true',
                        help='Re-evaluate PHP against immutable saved SciPy/mpmath values; do not regenerate reference fixtures')
    args = parser.parse_args()
    fixture = json.loads((ROOT / 'tests/fixtures/distributions-r.json').read_text())
    if args.php_results:
        php = json.loads(args.php_results.read_text())
    else:
        suffix = '.dylib' if sys.platform == 'darwin' else '.so'
        extension = ROOT / 'target/release' / f'libphit{suffix}'
        php = json.loads(subprocess.run(
            [os.environ.get('PHP', 'php'), '-n', '-d', f'extension={extension}',
             str(ROOT / 'docs/verifications/distributions.php')],
            check=True, capture_output=True, text=True).stdout)
    assert len(fixture['cases']) == len(php['results']), 'native case count differs'
    stored = {}
    if args.stored_references:
        for name in ('scipy','mpmath'):
            path = ROOT/f'tests/fixtures/distributions-{name}.json'
            stored[name] = json.loads(path.read_text())
            assert len(stored[name]['cases']) == len(fixture['cases'])
            for original, independent in zip(fixture['cases'], stored[name]['cases']):
                assert all(original[k] == independent[k] for k in ('distribution','params','operation','input'))
    scipy_cases, mpmath_cases, report_cases = [], [], []
    for case_index, (case, ours) in enumerate(zip(fixture['cases'], php['results'])):
        name, op, params = case['distribution'], case['operation'], case['params']
        ref = Reference(name, params)
        dist = scipy_distribution(name, params)
        inputs = [number(v) for v in case['input']]
        r_values = [number(v) for v in case['expected']]
        if args.stored_references:
            sp_values = [number(v) for v in stored['scipy']['cases'][case_index]['expected']]
        else:
            with np.errstate(all='ignore'):
                sp_values = [float(getattr(dist, SCIPY_METHOD[op])(x)) for x in inputs]
        php_values = [v if isinstance(v, dict) else number(v) for v in ours['values']]
        assert len(php_values) == len(inputs), 'native point count differs'
        if args.stored_references:
            references = [None if v is None else number(v) for v in stored['mpmath']['cases'][case_index]['expected']]
        else:
            references = []
            for i, x in enumerate(inputs):
                if op == 'quantile':
                    starts = [v for v in (php_values[i], r_values[i], sp_values[i]) if not isinstance(v, dict)]
                    reference = bounded(ref.quantile, x, starts)
                else:
                    reference = bounded(ref.value, op, mp.mpf(x))
                references.append(round_reference(reference))
        errors = {label: [error(v, r, op) for v, r in zip(values, references)]
                  for label, values in (('php', php_values), ('r', r_values), ('scipy', sp_values))}
        errors = {k: [encode(e) if e is not None and math.isinf(e) else e for e in v] for k, v in errors.items()}
        summary = {k: max((number(e) for e in v if e is not None), default=None) for k, v in errors.items()}
        missing = sum(r is None for r in references)
        scipy_cases.append(dict(distribution=name, params=params, operation=op, input=case['input'],
                                expected=[encode(v) for v in sp_values]))
        mpmath_cases.append(dict(distribution=name, params=params, operation=op, input=case['input'],
                                 expected=[encode(v) for v in references]))
        report_cases.append(dict(distribution=name, params=params, operation=op, n=len(inputs),
                                 references_unavailable=missing, max_error={k: encode(v) for k, v in summary.items()},
                                 errors=errors))
        fmt = lambda e: 'n/a' if e is None else f'{e:.1e}'
        print(f"{name:11} {op:9} {json.dumps(params):38} php={fmt(summary['php'])} r={fmt(summary['r'])} "
              f"scipy={fmt(summary['scipy'])} missing={missing}", flush=True)
    meta = dict(recorded_at=datetime.now(timezone.utc).isoformat(), generator='docs/verifications/compare_distributions.py')
    if not args.stored_references:
        (ROOT / 'tests/fixtures/distributions-scipy.json').write_text(json.dumps(dict(
            reference=f'SciPy {scipy.__version__}, NumPy {np.__version__}', **meta,
            conventions='scipy.stats frozen distributions; quantile is ppf; nonfinite numbers are strings',
            cases=scipy_cases), separators=(',', ':')) + '\n')
        (ROOT / 'tests/fixtures/distributions-mpmath.json').write_text(json.dumps(dict(
            reference=f'mpmath {mp.__version__} at {mp.mp.dps} digits', **meta,
            conventions='correctly rounded double of the exact value; null where no reference was computed',
            cases=mpmath_cases), separators=(',', ':')) + '\n')
    report = dict(**meta, platform=platform.platform(), php=php.get('php'), r=fixture['reference'],
                  scipy=scipy.__version__, numpy=np.__version__, mpmath=mp.__version__,
                  mpmath_digits=mp.mp.dps, method=__doc__.strip().split('\n\nOutputs:')[0],
                  error_definition='relative error vs the rounded mpmath reference; for log values and quantiles '
                                   'the denominator is max(|reference|, 1) and max(|reference|, 1e-300)',
                  cases=report_cases)
    if args.stored_references:
        report.update(native_refreshed_at=meta['recorded_at'],
            native_refresh_method='Re-evaluated every native scalar/batch against immutable saved R/SciPy/60-digit mpmath references. No reference values were recomputed.',
            reference_metadata={name:{k:v for k,v in data.items() if k!='cases'} for name,data in stored.items()},
            reference_sha256={name:hashlib.sha256((ROOT/f'tests/fixtures/distributions-{name}.json').read_bytes()).hexdigest() for name in ('r','scipy','mpmath')})
    report['tolerance_policy']='Original operation-specific PHP tolerances retained; independent mpmath adjudicates saved R disagreements. Documented boundary conventions have separate assertions.'
    (ROOT / 'docs/verifications/distributions-results.json').write_text(
        json.dumps(report, indent=1, default=encode) + '\n')
    print('Saved native comparison report' if args.stored_references else 'Saved fixtures and native comparison report')


if __name__ == '__main__':
    main()
