"""One benchmark run in a fresh process: python bench/fit.py WORKLOAD {data.csv|--baseline}

Times statsmodels, GPBoost (mixed-logistic) or SciPy (distributions). Prints one JSON line: elapsed ms and the
estimates. The runner measures memory, and records the lines between a workload's [name] and [/name] markers as its
timed code.
"""
import json
import sys
import time

import numpy as np
import pandas as pd

workload, path = sys.argv[1], sys.argv[2]
if workload == 'mixed-logistic':
    import gpboost as gpb
elif workload == 'distributions':
    from scipy import stats
else:
    import statsmodels.formula.api as smf
if path == '--baseline':
    sys.exit()

# Untimed: the data in a pandas DataFrame, or the input arrays.
if workload == 'distributions':
    n = 1_000_000
    i = np.arange(n)
    x = -5.0 + 10.0 * i / (n - 1)
    p = (i + 0.5) / n
    positive = 20.0 * (i + 0.5) / n
else:
    d = pd.read_csv(path, dtype={'g': str})
    if workload == 'mixed-logistic':
        X = np.column_stack([np.ones(len(d)), d[['x1', 'x2', 'x3']].to_numpy()])
        y = d['y'].to_numpy()

start = time.perf_counter()
if workload == 'ols':
    # [ols]
    fit = smf.ols(
        'y ~ x1 + x2 + x3 + x4 + x5 + x6 + x7 + x8 + x9 + x10',
        data=d,
    ).fit()
    coefficients, standard_errors = fit.params, fit.bse
    # [/ols]
elif workload == 'mixed':
    # [mixed]
    fit = smf.mixedlm(
        'y ~ x1 + x2 + x3',
        d,
        groups=d['g'],
    ).fit(reml=True)
    coefficients = fit.fe_params
    standard_errors = fit.bse_fe
    # [/mixed]
elif workload == 'logistic':
    # [logistic]
    fit = smf.logit(
        'y ~ x1 + x2 + x3 + x4 + x5 + x6 + x7 + x8 + x9 + x10',
        data=d,
    ).fit(method='newton', disp=0)
    coefficients, standard_errors = fit.params, fit.bse
    # [/logistic]
elif workload == 'probit':
    # [probit]
    fit = smf.probit(
        'y ~ x1 + x2 + x3 + x4 + x5 + x6 + x7 + x8 + x9 + x10',
        data=d,
    ).fit(method='newton', disp=0)
    coefficients, standard_errors = fit.params, fit.bse
    # [/probit]
elif workload == 'mixed-logistic':
    # [mixed-logistic]
    model = gpb.GPModel(
        group_data=d['g'],
        likelihood='bernoulli_logit',
    )
    model.set_optim_params(params={
        'optimizer_cov': 'lbfgs',
        'delta_rel_conv': 1e-12,
    })
    model.fit(y=y, X=X)
    table = model.get_coef(std_err=True)
    # [/mixed-logistic]
else:
    # [distributions]
    normal_cdf = stats.norm.cdf(x)
    normal_quantile = stats.norm.ppf(p)
    gamma_cdf = stats.gamma.cdf(positive, 2.5, scale=1.5)
    gamma_quantile = stats.gamma.ppf(p, 2.5, scale=1.5)
    # [/distributions]
elapsed = (time.perf_counter() - start) * 1000

if workload == 'distributions':
    values = np.concatenate([v[::1000] for v in (normal_cdf, normal_quantile, gamma_cdf, gamma_quantile)])
    print(json.dumps({'ms': elapsed, 'values': values.tolist()}))
elif workload == 'mixed-logistic':
    fixed = [c for c in table.columns if not c.startswith('Group')]
    print(json.dumps({'ms': elapsed, 'names': fixed, 'coefficients': table.loc['Param.', fixed].tolist(),
                      'std_errors': table.loc['Std. err.', fixed].tolist(), 'version': gpb.__version__}))
else:
    print(json.dumps({'ms': elapsed, 'names': list(coefficients.index), 'coefficients': list(coefficients),
                      'std_errors': list(standard_errors)}))
