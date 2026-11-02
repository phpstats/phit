"""High-precision reference values for the clean-room special-function specs, computed with mpmath at 60 digits.
Run from the repository root: .context/notebooks-venv/bin/python specs/cleanroom/generate-mpmath-tables.py"""
import json
import random

import mpmath as mp

mp.mp.dps = 60
random.seed(20261102)
OUT = 'specs/cleanroom/data/'


def s(x):
    return repr(float(x))


def stirling(n):
    n = mp.mpf(n)
    if n >= 1e6:
        # The defining formula cancels catastrophically here; the asymptotic series converges fast.
        return mp.fsum(mp.bernoulli(2 * k) / (2 * k * (2 * k - 1) * n ** (2 * k - 1)) for k in range(1, 12))
    return mp.loggamma(n + 1) - (n + mp.mpf(1) / 2) * mp.log(n) + n - mp.log(mp.sqrt(2 * mp.pi))


def deviance(x, np_):
    x, np_ = mp.mpf(x), mp.mpf(np_)
    return (x * mp.log(x / np_) if x > 0 else mp.mpf(0)) + np_ - x


def ln_pois(x, lam):
    x, lam = mp.mpf(x), mp.mpf(lam)
    return x * mp.log(lam) - lam - mp.loggamma(x + 1)


def ln_binom(x, n, p):
    x, n, p = mp.mpf(x), mp.mpf(n), mp.mpf(p)
    return mp.loggamma(n + 1) - mp.loggamma(x + 1) - mp.loggamma(n - x + 1) + x * mp.log(p) + (n - x) * mp.log(1 - p)


tables = {}
ns = [k / 2 for k in range(1, 31)] + [random.uniform(0.01, 15) for _ in range(60)] + \
     [10 ** random.uniform(1, 12) for _ in range(60)] + [15.0, 35.0, 80.0, 500.0, 15.5, 35.5, 80.5, 500.5, 1e15, 1e300]
tables['stirling_error'] = [[s(n), s(stirling(n))] for n in ns]

dev = []
for _ in range(200):
    np_ = 10 ** random.uniform(-3, 8)
    x = np_ * random.choice([random.uniform(0, 3), 1 + random.uniform(-1e-3, 1e-3), 1 + random.uniform(-1e-9, 1e-9)])
    x = float(x)
    dev.append([s(x), s(np_), s(deviance(x, np_))])
dev += [[s(0.0), s(2.5), s(deviance(0.0, 2.5))], [s(7.0), s(7.0), s(0)]]
tables['deviance_term'] = dev

pois = []
for _ in range(200):
    lam = 10 ** random.uniform(-3, 7)
    x = float(round(lam * random.uniform(0, 3))) if random.random() < 0.7 else random.uniform(0, 3 * lam)
    pois.append([s(x), s(lam), s(ln_pois(x, lam))])
tables['ln_poisson_raw'] = pois

binom = []
for _ in range(200):
    n = float(round(10 ** random.uniform(0, 7)))
    p = random.choice([random.uniform(0, 1), 10 ** random.uniform(-8, -1), 1 - 10 ** random.uniform(-8, -1)])
    x = float(round(random.uniform(0, n)))
    binom.append([s(x), s(n), s(p), s(1 - mp.mpf(p)), s(ln_binom(x, n, p))])
tables['ln_binomial_raw'] = binom

lower = []
for _ in range(200):
    a = 10 ** random.uniform(-3, 5)
    x = random.uniform(0, a + 1) * random.uniform(0.001, 0.999)
    lower.append([s(a), s(x), s(mp.log(mp.gammainc(a, 0, x, regularized=True)))])
tables['ln_gamma_lower'] = lower

upper = []
for _ in range(200):
    a = 10 ** random.uniform(-3, 5)
    x = (a + 1) * random.uniform(1.001, 50)
    upper.append([s(a), s(x), s(mp.log(mp.gammainc(a, x, mp.inf, regularized=True)))])
tables['ln_gamma_upper'] = upper

beta = []
for _ in range(200):
    a, b = 10 ** random.uniform(-2, 4), 10 ** random.uniform(-2, 4)
    x = (a + 1) / (a + b + 2) * random.uniform(0.001, 0.999)
    beta.append([s(a), s(b), s(x), s(mp.log(mp.betainc(a, b, 0, x, regularized=True)))])
tables['ln_beta_lower'] = beta

# x below the double range: the caller passes x = 0 (underflowed) with ln x finite, and ln(1 - x) = 0.
tiny = []
for _ in range(60):
    a, b = 10 ** random.uniform(-2, 3), 10 ** random.uniform(-2, 3)
    ln_x = random.uniform(-2000, -700)
    with mp.workdps(80):
        x = mp.exp(ln_x)
        value = mp.log(mp.betainc(a, b, 0, x, regularized=True))
    tiny.append([s(a), s(b), s(float(x)), s(ln_x), s(0.0), s(value)])
tables['ln_beta_lower_tiny'] = tiny

with open(OUT + 'special.json', 'w') as f:
    json.dump(tables, f)
print({k: len(v) for k, v in tables.items()})
