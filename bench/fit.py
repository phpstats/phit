"""One benchmark run in a fresh process: python bench/fit.py {ols|mixed} {data.csv|--baseline}

Prints one JSON line: elapsed ms, coefficients and standard errors. The runner measures memory.
"""
import json
import sys
import time

import pandas as pd
import statsmodels
import statsmodels.formula.api as smf


model, path = sys.argv[1], sys.argv[2]
if path == '--baseline':
    sys.exit()

# Untimed: read the CSV into a pandas DataFrame.
d = pd.read_csv(path, dtype={'g': str})

start = time.perf_counter()
if model == 'ols':
    fit = smf.ols('y ~ x1 + x2 + x3 + x4 + x5 + x6 + x7 + x8 + x9 + x10', data=d).fit()
    coefficients, standard_errors = fit.params, fit.bse
else:
    fit = smf.mixedlm('y ~ x1 + x2 + x3', d, groups=d['g']).fit(reml=True)
    coefficients, standard_errors = fit.fe_params, fit.bse_fe
elapsed = (time.perf_counter() - start) * 1000

print(json.dumps({
    'ms': elapsed, 'names': list(coefficients.index), 'coefficients': list(coefficients),
    'std_errors': list(standard_errors), 'version': statsmodels.__version__,
    'method': getattr(fit, 'method', None), 'converged': bool(getattr(fit, 'converged', True)),
}))
