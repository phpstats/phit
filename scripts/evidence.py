"""Benchmark and verification records for phpstats.org, in the format of phpstats.org's docs/evidence-records.md.

Every record is built from results already saved under docs/; nothing here fits or times anything. Case status
comes from the saved values and the rule each comparison script enforces, so a record shows a failure if the
saved values would not pass.
"""
import json
import math
import re
import statistics
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
GITHUB = 'https://github.com/phpstats/phit/blob/main/'


def load(path):
    return json.loads((ROOT / path).read_text())


def pairs(actual, expected):
    """(package, reference) number pairs in reference order, matching dicts by the reference's keys.

    A reference entry of None (JSON null) pairs with whatever the package returned. Returns None when the
    shapes differ.
    """
    if isinstance(expected, dict):
        if not isinstance(actual, dict) or any(k not in actual for k in expected):
            return None
        out = []
        for key, value in expected.items():
            inner = pairs(actual[key], value)
            if inner is None:
                return None
            out += inner
        return out
    if isinstance(expected, list):
        if not isinstance(actual, list) or len(actual) != len(expected):
            return None
        out = []
        for a, e in zip(actual, expected):
            inner = pairs(a, e)
            if inner is None:
                return None
            out += inner
        return out
    if isinstance(expected, (bool, str)):
        return []
    return [(actual, expected)]


def compare(actual, expected, atol, rtol):
    """Compare a package value with a reference under |x - r| <= atol + rtol |r|.

    Returns n_compared, the largest absolute error, the number of failing values, and the number of reference
    entries that are undefined (null) and were not compared.
    """
    matched = pairs(actual, expected)
    if matched is None:
        n = len(pairs(expected, expected) or [])
        return {'n': n, 'max_abs': None, 'fails': max(n, 1), 'undefined': 0, 'shape': False}
    xs, rs = [a for a, _ in matched], [e for _, e in matched]
    n = worst = fails = undefined = 0
    for x, r in zip(xs, rs):
        if r is None:
            undefined += 1
            continue
        n += 1
        if x is None or not math.isfinite(x):
            fails += 1
            continue
        error = abs(x - r)
        worst = max(worst, error)
        if error > atol + rtol * abs(r):
            fails += 1
    return {'n': n, 'max_abs': worst, 'fails': fails, 'undefined': undefined, 'shape': True}


def case_row(name, quantity, result, rule, actual=None, expected=None, peers=None):
    row = {'name': name, 'quantity': quantity.replace('_', ' '), 'rule': rule,
           'status': 'pass' if result['fails'] == 0 else 'fail', 'n_compared': result['n'],
           'max_abs_error': result['max_abs']}
    if result['fails']:
        row['reason'] = (f"{result['fails']} of {result['n']} values outside the tolerance" if result['shape']
                         else 'phit returned a different number of values than the reference')
    if result['undefined']:
        row['note'] = f"{result['undefined']} reference values are undefined in R (null) and not compared"
    if isinstance(expected, (int, float)) and not isinstance(expected, bool):
        row['actual'], row['expected'] = actual, expected
    if peers:
        row['peers'] = peers
    return row


def run_environment():
    run = load('docs/verifications/run-environment.json')
    return {'date': run['recorded_at'][:10], 'commit': run['commit'], 'php': run['php'], 'os': run['os'],
            'phit': run['phit']}


def split_versions(versions):
    out = {}
    for key, value in versions.items():
        match = re.match(r'R version (\S+)', str(value))
        out[key] = match[1] if match else str(value)
    return out


# Verification ---------------------------------------------------------------------------------------------------

GLM_DIFFERENCES = [
    'R intervals come from confint.default() (Wald), not profile-likelihood confint().',
    'phit reports the covariance at the converged coefficients; R glm() evaluates it at the last IRLS iterate. With '
    'the tight fitting tolerances used here the difference is negligible. See the R compatibility page for details.',
]
FIXED = {
    'ols': dict(title='OLS', peers='statsmodels', reference='R lm(), summary(), confint(), logLik()',
                inputs='Seven designs: the simulated housing sample with one and two predictors (docs/data/housing.csv), '
                       'saved R fixtures for simple, multiple, no-intercept, and correlated designs '
                       '(tests/fixtures/ols-r.json), and a listwise-deletion case (6 rows, 4 retained).',
                data='docs/data/housing.csv',
                exclusions=['Perfect fits compare coefficients, predictions, residuals, and standard errors only: their '
                            't statistics, p-values, R², and likelihood are not finite or not stable.',
                            'R reports undefined quantities as NA; those entries are not compared.'],
                known=['phit rejects a rank-deficient design; R lm() drops aliased columns and reports NA.',
                       'Without an intercept, R² is uncentered, as in R.']),
    'logistic': dict(title='Logistic regression', peers='statsmodels', reference='R glm(family = binomial)',
                     inputs='Six saved R fixtures (tests/fixtures/logistic-r.json): the simulated learning sample '
                            '(docs/data/learning.csv), one predictor, no intercept, intercept only, correlated columns, '
                            'and imbalanced outcomes, plus a listwise-deletion case with two dropped rows.',
                     data='docs/data/learning.csv',
                     exclusions=['Separated and quasi-separated designs are rejected and tested separately in '
                                 'tests/logistic.php, not compared with R.'],
                     known=GLM_DIFFERENCES + ['Without an intercept, the null deviance uses probability 0.5.']),
    'probit': dict(title='Probit regression', peers='statsmodels', reference='R glm(family = binomial(link = "probit"))',
                   inputs='Six saved R fixtures (tests/fixtures/probit-r.json): the simulated learning sample, one '
                          'predictor, no intercept, intercept only, correlated columns, and imbalanced outcomes, plus a '
                          'listwise-deletion case with two dropped rows.',
                   data='docs/data/learning.csv',
                   exclusions=['Separated and quasi-separated designs are rejected and tested separately in '
                               'tests/probit.php, not compared with R.'],
                   known=GLM_DIFFERENCES + ['The covariance is the inverse expected information, as in glm(); a '
                                            'discrete-model probit fit with observed-Hessian covariance can differ.']),
    'poisson': dict(title='Poisson regression', peers='statsmodels', reference='R glm(family = poisson)',
                    inputs='Six saved R fixtures (tests/fixtures/poisson-r.json): the simulated clinic sample '
                           '(docs/data/clinic.csv), one predictor, no intercept, intercept only, correlated columns, '
                           'and sparse counts, plus a listwise-deletion case with two dropped rows.',
                    data='docs/data/clinic.csv',
                    exclusions=['These cases have no offset; offset and exposure fits are covered by the test-suite '
                                'record (tests/intercept.php and tests/r-compatibility.php).'],
                    known=GLM_DIFFERENCES + ['Without an intercept, the null deviance uses a mean of one.']),
}


def fixed_verification(slug):
    spec = FIXED[slug]
    results = load(f'docs/verifications/{slug}-results.json')
    atol, rtol = 1e-9, 1e-7
    cases = []
    for record in results:
        for quantity, errors in record['max_absolute_errors_vs_R'].items():
            if 'phit' not in errors or quantity not in record['R']:
                continue
            result = compare(record['phit'][quantity], record['R'][quantity], atol, rtol)
            cases.append(case_row(record['name'], quantity, result, 'model', record['phit'][quantity],
                                  record['R'][quantity], {k: v for k, v in errors.items() if k != 'phit'}))
    command = f'.context/notebooks-venv/bin/python docs/verifications/compare_{slug}.py'
    return {
        'title': f"{spec['title']}: agreement with R",
        'label': spec['title'],
        'kind': 'reference',
        'checked': f"The matrix fit's coefficients, covariance, standard errors, test statistics, p-values, intervals, "
                   f"fitted values, residuals, and fit statistics on the same explicit design as R and "
                   f"{spec['peers']}, with missing rows dropped the same way.",
        'reference': spec['reference'],
        'versions': split_versions(results[0]['versions']),
        'inputs': spec['inputs'],
        'comparison': [{'id': 'model', 'applies_to': 'every compared value', 'kind': 'tolerance', 'atol': atol,
                        'rtol': rtol},
                       {'id': 'rows', 'applies_to': 'retained and dropped row positions, counts, and warnings',
                        'kind': 'exact'}],
        'cases': cases,
        'exclusions': spec['exclusions'],
        'known_differences': spec['known'],
        'reproduce': ['cargo build -p phit --release --locked', command],
        'source': f'docs/verifications/{slug}-results.json',
        'data': spec['data'],
        'environment': run_environment(),
        'group': 'Models',
    }


def mixed_verification(slug):
    results = load(f'docs/verifications/{slug}-results.json')
    public = slug == 'lmm-public'
    cases, rules = [], {}
    for record in results:
        policy = record.get('tolerance') or {'absolute': 1e-7, 'relative': 1e-6}
        for quantity, errors in record['max_absolute_errors_vs_R'].items():
            if 'phit' not in errors or quantity not in record['R']:
                continue
            atol = policy.get('quantity_absolute', {}).get(quantity, policy['absolute'])
            rtol = policy.get('quantity_relative', {}).get(quantity, policy['relative'])
            rule = f'atol {atol:g}, rtol {rtol:g}'
            rules[rule] = (atol, rtol)
            result = compare(record['phit'][quantity], record['R'][quantity], atol, rtol)
            peers = {('statsmodels' if k == 'Python' else k): v for k, v in errors.items() if k != 'phit'}
            cases.append(case_row(record['name'], quantity, result, rule, record['phit'][quantity],
                                  record['R'][quantity], peers))
    comparison = [{'id': rule, 'applies_to': 'quantities this tolerance names in the cases', 'kind': 'tolerance',
                   'atol': atol, 'rtol': rtol} for rule, (atol, rtol) in rules.items()]
    command = '.context/notebooks-venv/bin/python docs/verifications/compare_lmm.py' + (' --public' if public else '')
    known = ['statsmodels estimates are combined with plug-in GLS covariance to match lme4; statsmodels\' own '
             'joint-Hessian covariance is a different convention.',
             'The profile search uses a finite log-ratio range and grid; agreement does not prove a global optimum.']
    if public:
        known.append('nlme is a second reference for the same ML fits.')
    return {
        'title': ('Mixed models on published data: agreement with lme4' if public
                  else 'Gaussian random intercept: agreement with lme4'),
        'label': 'Published data' if public else 'Random intercept',
        'kind': 'reference',
        'checked': ('ML fits of sleepstudy, Orthodont, Machines, and Hsb82 against lme4 and nlme, with statsmodels.'
                    if public else
                    'ML fits of one Gaussian random intercept: coefficients, inference, fitted values, residuals, '
                    'group effects, variance components, and likelihood, with missing rows dropped the same way.'),
        'reference': 'R lme4 lmer(REML = FALSE, bobyqa rhoend = 1e-10)' + (' and nlme lme()' if public else ''),
        'versions': split_versions(results[0]['versions']),
        'inputs': ('Four published data sets, with provenance and licenses in docs/data/lmm '
                   '(tests/fixtures/lmm-public-r.json).' if public else
                   'Saved lme4 fixtures (tests/fixtures/lmm-r.json): simulated schools with correlated predictors '
                   '(docs/data/schools.csv), one predictor, no fixed intercept, intercept only, a singleton group, '
                   'and a zero group-variance boundary, plus missing values in predictors, outcomes, and group IDs.'),
        'comparison': comparison,
        'cases': cases,
        'exclusions': ['REML, random slopes, and several grouping terms are covered by the R compatibility record.'],
        'known_differences': known,
        'reproduce': ['cargo build -p phit --release --locked', command],
        'source': f'docs/verifications/{slug}-results.json',
        'data': 'docs/data/lmm' if public else 'docs/data/schools.csv',
        'environment': run_environment(),
        'group': 'Models',
    }


def logistic_mixed_verification():
    data = load('docs/verifications/logistic-mixed-results.json')
    t = data['tolerances']
    inference = {'covariance', 'std_errors', 'z_statistics', 'p_values', 'conf_int'}
    likelihood = {'log_likelihood', 'neg2_log_likelihood', 'aic', 'bic'}
    cases = []
    for record in data['results']:
        for quantity, expected in record['R'].items():
            if isinstance(expected, bool) or quantity not in record['PHP']:
                continue
            group = 'inference' if quantity in inference else ('likelihood' if quantity in likelihood else 'estimates')
            result = compare(record['PHP'][quantity], expected, t[group]['absolute'], t[group]['relative'])
            cases.append(case_row(record['name'], quantity, result, group, record['PHP'][quantity], expected))
        same = record['scipy_same_approximation_log_likelihood']
        error = abs(same - record['PHP']['log_likelihood'])
        cases.append({'name': record['name'], 'quantity': 'log likelihood vs independent SciPy evaluation',
                      'rule': 'independent', 'status': 'pass' if error < t['independent_likelihood_absolute'] else 'fail',
                      'actual': record['PHP']['log_likelihood'], 'expected': same, 'max_abs_error': error, 'n_compared': 1})
    comparison = [{'id': g, 'applies_to': text, 'kind': 'tolerance', 'atol': t[g]['absolute'], 'rtol': t[g]['relative']}
                  for g, text in [('estimates', 'coefficients, random effects, variance components, fitted values'),
                                  ('inference', 'covariance, standard errors, z, p, intervals'),
                                  ('likelihood', 'log likelihood, deviance, AIC, BIC')]]
    comparison.append({'id': 'independent', 'kind': 'tolerance', 'atol': t['independent_likelihood_absolute'],
                       'applies_to': "phit's log likelihood against SciPy's evaluation of the same approximation"})
    return {
        'title': 'Binomial random intercept: agreement with lme4', 'label': 'Binomial mixed', 'kind': 'reference',
        'checked': 'fit_mixed_logistic_matrix() against lme4 glmer() at the Laplace approximation and 9, 25, and 61 '
                   'adaptive quadrature points, with an independent SciPy evaluation of the same likelihood.',
        'reference': 'R lme4 glmer(family = binomial), bobyqa, tolPwrss 1e-12',
        'versions': split_versions(data['versions']),
        'inputs': 'Seven data sets (tests/fixtures/logistic-mixed-r.json), including lme4\'s cbpp, rare events, '
                  'grouped outcomes with offsets, large group variation, and a zero-variance boundary.',
        'comparison': comparison, 'cases': cases,
        'exclusions': ['statsmodels\' Bayesian binomial mixed estimator has a different objective and is not compared.'],
        'known_differences': ['lme4\'s default tolPwrss (1e-7) leaves its conditional modes slightly short of '
                              'convergence; the references use tolPwrss 1e-12, where lme4 and phit agree.',
                              'Agreement with lme4 is not integration accuracy: integration error is measured '
                              'separately with QUADPACK and reported in the source results.'],
        'reproduce': ['cargo build -p phit --release --locked',
                      '.context/notebooks-venv/bin/python docs/verifications/compare_logistic_mixed.py'],
        'source': 'docs/verifications/logistic-mixed-results.json', 'data': 'tests/fixtures/logistic-mixed-r.json',
        'environment': run_environment(), 'group': 'Models',
    }


def r_compatibility_verification():
    data = load('docs/verifications/r-compatibility-results.json')
    t = data['tolerances']
    cases = []
    for record in data['fixed']:
        for quantity, expected in record['R'].items():
            if isinstance(expected, bool) or quantity not in record['PHP']:
                continue
            result = compare(record['PHP'][quantity], expected, t['fixed']['absolute'], t['fixed']['relative'])
            cases.append(case_row(f"{record['name']} ({record['model']})", quantity, result, 'fixed',
                                  record['PHP'][quantity], expected))
    for record in data['mixed']:
        for optimizer, reference in record['R'].items():
            for backend, fit in record['PHP'].items():
                for quantity, expected in reference['result'].items():
                    if isinstance(expected, bool) or quantity not in fit:
                        continue
                    if quantity in ('log_likelihood', 'aic', 'bic'):
                        rule = 'mixed_likelihood'
                    else:
                        rule = 'mixed_bobyqa' if (optimizer == 'bobyqa' and backend == 'bobyqa') else 'mixed_other_optimizers'
                    actual = fit[quantity]
                    result = compare(actual, expected, t[rule]['absolute'], t[rule]['relative'])
                    cases.append(case_row(f"{record['name']} ({record['method']})",
                                          f'phit {backend} vs lme4 {optimizer}: {quantity}', result, rule, actual, expected))
    comparison = [{'id': rule, 'kind': 'tolerance', 'atol': v['absolute'], 'rtol': v['relative'], 'applies_to': text}
                  for rule, v in t.items() for text in [{
                      'fixed': 'weighted and offset GLMs against base R',
                      'mixed_bobyqa': 'mixed-model estimates, phit bobyqa against lme4 bobyqa',
                      'mixed_other_optimizers': 'mixed-model estimates for other optimizer pairs',
                      'mixed_likelihood': 'mixed-model log likelihood, AIC, and BIC'}[rule]]]
    return {
        'title': 'Weights, offsets, and mixed models: agreement with R and lme4', 'label': 'R compatibility',
        'kind': 'reference',
        'checked': 'Weighted and offset GLMs against base R, and Gaussian mixed models (REML and ML, random slopes, '
                   'several terms, nesting) against all four lme4 optimizers with both phit optimizers.',
        'reference': 'R glm(), lm(); lme4 lmer() with bobyqa, Nelder_Mead, nlminbwrap, and nloptwrap',
        'versions': {k: (re.match(r'R version (\S+)', v)[1] if k == 'R' and v.startswith('R version') else str(v).strip())
                     for k, v in data['versions'].items()},
        'inputs': 'tests/fixtures/r-compatibility.json (fixed models) and tests/fixtures/lmm-expanded-r.json '
                  '(mixed models, including sleepstudy).',
        'comparison': comparison, 'cases': cases,
        'exclusions': ['(x || g) terms compare only the conditional-variance diagonals, as lme4 reports them.'],
        'known_differences': ['Independent optimizers stop at slightly different points on flat likelihoods; '
                              'optimizer pairs other than bobyqa/bobyqa use the looser rule.'],
        'reproduce': ['cargo build -p phit --release --locked',
                      '.context/notebooks-venv/bin/python docs/verifications/compare_r_compatibility.py'],
        'source': 'docs/verifications/r-compatibility-results.json', 'environment': run_environment(),
        'group': 'Models',
    }


def distributions_verification():
    data = load('docs/verifications/distributions-results.json')
    cases = []
    for c in data['cases']:
        errors = c['max_error']
        params = ', '.join(f'{k}={v}' for k, v in c['params'].items())
        php = errors.get('php')
        log_or_quantile = c['operation'].startswith('log') or c['operation'] == 'quantile'
        row = {'name': f"{c['distribution']}({params})", 'quantity': c['operation'], 'n_compared': c['n'],
               'rule': 'log-and-quantile' if log_or_quantile else 'probability', 'max_rel_error': php,
               'peers': {k: v for k, v in errors.items() if k != 'php'},
               'status': 'pass' if php is not None and php <= 1e-12 else 'fail'}
        if row['status'] == 'fail':
            row['reason'] = 'relative error above 1e-12' if php is not None else 'no phit value'
        if c.get('references_unavailable'):
            row['note'] = f"{c['references_unavailable']} points have no mpmath reference and are not compared"
        cases.append(row)
    return {
        'title': 'Distributions: accuracy against 60-digit references', 'label': 'Distributions', 'kind': 'reference',
        'checked': 'Densities, masses, CDFs, survival functions, their logs, and quantiles of every family at saved '
                   'points, against mpmath at 60 digits, with R and SciPy as peers.',
        'reference': f"mpmath {data.get('mpmath')} at {data.get('mpmath_digits', 60)} digits, each value rounded once",
        'versions': {'mpmath': data.get('mpmath'), 'R': data.get('r'), 'SciPy': data.get('scipy'),
                     'NumPy': data.get('numpy'), 'PHP': data.get('php')},
        'inputs': 'tests/fixtures/distributions-r.json: the cases and points; references in '
                  'tests/fixtures/distributions-mpmath.json.',
        'comparison': [{'id': 'probability', 'kind': 'tolerance', 'rtol': 1e-12, 'rel_floor': 1e-300,
                        'applies_to': 'densities, masses, CDFs, and survival functions'},
                       {'id': 'log-and-quantile', 'kind': 'tolerance', 'rtol': 1e-12, 'rel_floor': 1,
                        'applies_to': 'log values and quantiles'}],
        'cases': cases,
        'exclusions': ['Points where mpmath has no reference are counted per case and not compared.',
                       'Documented boundary conventions are asserted separately in tests/distributions.php.'],
        'known_differences': [
            'For a point mass, the discrete quantile at p = 1 is the mass location; R and SciPy may return the upper '
            'support bound.',
            "R's F density at +INF is NAN; phit returns 0.",
            'R F quantiles can round to zero for tiny probabilities and small numerator df.',
            "R's Student-t quantile at df = 0.5 has a small nonzero center and some far-tail inaccuracy.",
            "R's discrete-quantile adjustment near 1 − 2^-52 can shift the count; phit uses a relative 64·EPSILON "
            'allowance on the smaller tail.',
            "R's mean-form negative binomial mass differs from the high-precision value when size is much larger "
            'than the count.',
            "SciPy's large-df chi-square CDF has about 8e-9 relative error at the saved lower-tail point."],
        'reproduce': ['cargo build -p phit --release --locked',
                      '.context/notebooks-venv/bin/python docs/verifications/compare_distributions.py'],
        'source': 'docs/verifications/distributions-results.json', 'environment': run_environment(),
        'group': 'Numerics',
    }


SUITES = {
    'ols': ('OLS against saved R lm() fixtures', 'tests/fixtures/ols-r.json', 'model'),
    'logistic': ('Logistic regression against saved R glm() fixtures', 'tests/fixtures/logistic-r.json', 'model'),
    'probit': ('Probit regression against saved R glm() fixtures', 'tests/fixtures/probit-r.json', 'model'),
    'poisson': ('Poisson regression against saved R glm() fixtures', 'tests/fixtures/poisson-r.json', 'model'),
    'aliases': ('Deprecated aliases return their replacement\'s result', None, 'exact'),
    'formula': ('Formula designs against R model.matrix(), and lm()/glm() fits', 'tests/fixtures/formula-r.json', 'formula'),
    'formula-mixed': ('Formula mixed models against lme4 lmer() and glmer()', 'tests/fixtures/formula-lme4-r.json', 'mixed'),
    'predict': ('predict() against R predict()', 'tests/fixtures/predict-r.json', 'predict'),
    'lmm': ('Random intercept against saved lme4 fixtures', 'tests/fixtures/lmm-r.json', 'mixed'),
    'r-compatibility': ('Weighted and offset GLMs against base R', 'tests/fixtures/r-compatibility.json', 'r-compat'),
    'lmm-expanded': ('Mixed-model structures against four lme4 optimizers', 'tests/fixtures/lmm-expanded-r.json', 'mixed'),
    'logistic-mixed': ('Binomial random intercept against lme4 glmer()', 'tests/fixtures/logistic-mixed-r.json', 'mixed'),
    'stability': ('Result contracts and exception categories', None, 'exact'),
    'intercept': ('Automatic intercepts and Poisson exposure offsets', None, 'intercept'),
    'distributions': ('Distribution functions against R and mpmath', 'tests/fixtures/distributions-r.json', 'distributions'),
    'descriptive': ('Descriptive statistics against R', 'tests/fixtures/descriptive-r.json', 'relative'),
    'smoothing': ('Kernel densities, bandwidths, interpolation, and ECDF against R', 'tests/fixtures/smoothing-r.json', 'relative'),
    'optimize': ('optimize() and uniroot() against R', 'tests/fixtures/optimize-r.json', 'exact'),
    'rng': ('Seeded sampling against independent ChaCha streams and R quantiles', 'tests/fixtures/rng-python-r.json', 'rng'),
    'draws': ('Posterior-draw kernels against 80-digit Python references', 'tests/fixtures/draws-python.json', 'draws'),
    'csv': ('CSV reader: parsing rules, options, types, and errors', None, 'exact'),
    'csv-conformance': ("CSV reader against phibble's reader on a fixed corpus", 'tests/fixtures/csv-conformance.json', 'exact'),
    'csv-phibble': ("CSV reader against phibble's PHP reader on generated files", None, 'exact'),
    'extreme-inputs': ('Extreme but identified inputs against base R', 'tests/fixtures/stress-r.json', 'stress'),
}


def test_suites_verification():
    data = load('docs/verifications/test-suites-results.json')
    cases = []
    for suite in data['suites']:
        title, fixture, rule = SUITES[suite['suite']]
        counts = re.findall(r'(\d+) (?:R |bit-for-bit R |independent R/ChaCha |)(?:comparisons|values|checks|cases)',
                            suite['summary'])
        row = {'name': title, 'quantity': suite['summary'], 'rule': rule, 'status': suite['status']}
        if counts:
            row['n_compared'] = sum(int(n) for n in counts)
        if suite['status'] != 'pass':
            row['reason'] = suite['summary'] if suite['status'] == 'skipped' else (suite.get('error') or 'failed')[:300]
        if fixture:
            row['note'] = fixture
        cases.append(row)
    rules = {
        'model': ('saved R fixtures for OLS and GLMs', 1e-9, 1e-7),
        'formula': ('model.matrix values (columns, terms, and rows exact); lm/glm fits use atol 1e-9, rtol 1e-7', 1e-12, 1e-12),
        'mixed': ('lme4 estimates with bobyqa; likelihood 5e-6 + 1e-9·|r|, glmer as in the binomial record', 1e-5, 1e-5),
        'predict': ('fits, intervals, and standard errors; mixed models as their fits', 1e-7, 1e-6),
        'r-compat': ('weighted and offset GLMs', 1e-7, 1e-6),
        'intercept': ('the exposure intercept and its SE (likelihood and counts at 1e-9); identical results with and '
                      'without automatic intercepts', 1e-10, None),
        'distributions': ('densities, CDFs, and survival functions (logs relative to max(1, |r|), quantiles 1e-15 + '
                          '1e-12·|r|)', 1e-300, 1e-12),
        'relative': ('descriptive statistics and smoothing; exact zeros, ECDF counts, and type-7 identities exact', None, 1e-12),
        'rng': ('R inverse-CDF values; ChaCha uniforms and discrete samples exact', 1e-15, 1e-12),
        'draws': ('moments, covariance maps, affine draws, and log determinants', 1e-11, 1e-11),
        'stress': ('per-case tolerances stored in tests/fixtures/stress-r.json, scaled with the response', None, None),
    }
    comparison = [{'id': 'exact', 'kind': 'exact',
                   'applies_to': 'results, row accounting, parsed values, types, and errors that must be identical'}]
    for rule_id, (text, atol, rtol) in rules.items():
        rule = {'id': rule_id, 'kind': 'tolerance', 'applies_to': text}
        if atol is not None:
            rule['atol'] = atol
        if rtol is not None:
            rule['rtol'] = rtol
        comparison.append(rule)
    return {
        'title': 'Test suites: saved references and contracts', 'label': 'Test suites', 'kind': 'reference',
        'checked': 'Every PHP test suite that scripts/check runs, against saved references from R, lme4, mpmath, '
                   'Python, and phibble, and against phit\'s own contracts. Each row is one suite and its own summary.',
        'reference': 'Saved fixtures under tests/fixtures; each lists its generator and versions',
        'inputs': [{'name': 'Fixture provenance', 'detail': 'generators, versions, hashes, and tolerance policies',
                    'url': GITHUB + 'docs/verifications/reference-manifest.json'}],
        'comparison': comparison, 'cases': cases,
        'reproduce': ['cargo build -p phit --release --locked',
                      'PHP=$(command -v php) python3 docs/verifications/run_test_suites.py'],
        'source': 'docs/verifications/test-suites-results.json',
        'environment': {'date': data['recorded_at'][:10], 'commit': data['commit'], 'php': data['php'],
                        'os': data['os'], 'phit': data['phit']},
        'group': 'Test suites',
    }


# Benchmarks -----------------------------------------------------------------------------------------------------

LABELS = {'PHP': 'phit', 'R': 'R', 'statsmodels': 'statsmodels', 'SciPy reference': 'SciPy',
          'PHP_scalar': 'phit (scalar calls)', 'PHP_batch': 'phit (one batch call)', 'Rust': 'Rust core',
          'SciPy': 'SciPy'}
MEMORY_METHOD = ('Peak of the process\'s anonymous resident memory (RssAnon in /proc), sampled every millisecond by '
                 'bench/peakmem.c; the peak resident set size, file pages included, from getrusage.')
R_TICK = 0.001


def mib(value):
    return round(value / 2 ** 20, 1) if value is not None else None


def bench_environment(data, php=None, versions=None, threads=1):
    return {'cpu': data.get('cpu'), 'cores': data.get('cpu_count'),
            'memory_gb': round(data['host_memory_bytes'] / 2 ** 30, 1) if data.get('host_memory_bytes') else None,
            'os': data.get('platform'), 'php': php,
            'versions': {k: v for k, v in (versions or {}).items() if v}, 'threads': threads, 'cache': 'warm',
            'repeats': data.get('repeats'), 'warmups': data.get('warmups'),
            'date': (data.get('recorded_at') or '')[:10] or None, 'commit': data.get('commit')}


def timing_row(case, label, reps_seconds, result, digits=3, r_timer=False):
    reps = [round(r * 1000, digits) for r in reps_seconds]
    median = statistics.median(reps_seconds)
    row = {'case': case, 'implementation': label, 'value': round(median * 1000, digits), 'reps': reps,
           'peak_memory_mib': mib(result.get('peak_memory_bytes')), 'peak_rss_mib': mib(result.get('peak_rss_bytes'))}
    if r_timer:
        if median <= R_TICK * 1.000001:
            row['value'] = None
            row['note'] = "below the 1 ms resolution of R's timer (proc.time() counts whole milliseconds)"
        elif median < 10 * R_TICK:
            row['note'] = "R's timer counts whole milliseconds, so this value is within a few ticks of its resolution"
    return row


MODEL_BENCH = {
    'ols': ('OLS', 'fit_ols_matrix()', 'lm.fit() with the full summary', 'statsmodels OLS'),
    'logistic': ('Logistic regression', 'fit_logistic_matrix()', 'glm(family = binomial)', 'statsmodels GLM Binomial'),
    'probit': ('Probit regression', 'fit_probit_matrix()', 'glm(family = binomial(link = "probit"))', 'statsmodels GLM Binomial probit'),
    'poisson': ('Poisson regression', 'fit_poisson_matrix()', 'glm(family = poisson)', 'statsmodels GLM Poisson'),
    'lmm': ('Gaussian random intercept', 'fit_mixed_gaussian_intercept_matrix()', 'lme4 lmer(REML = FALSE, bobyqa)',
            'statsmodels MixedLM (ML)'),
}


def model_benchmark(slug):
    data = load(f'docs/benchmarks/{slug}-results.json')
    title, phit, r, py = MODEL_BENCH[slug]
    rows, check = [], {}
    first = data['cases'][0]['implementations']
    for case in data['cases']:
        label = f"n = {case['n']:,}, p = {case['p']}"
        for name in ['PHP', 'statsmodels', 'R']:
            result = case['implementations'][name]
            rows.append(timing_row(label, LABELS[name], result['fit_seconds'], result, r_timer=name == 'R'))
        for name, errors in case['max_absolute_errors_vs_R'].items():
            worst = max(errors.values())
            key = f"max_abs_difference_vs_R_{LABELS[name]}"
            check[key] = max(check.get(key, 0), worst)
    tol = data['tolerance']
    check['tolerance'] = f"{tol['absolute']:g} + {tol['relative']:g}·|R|" + (
        '; variance-sensitive quantities and random effects rtol 1e-5; statsmodels as a peer at 1e-6 + 1e-5·|R|'
        if slug == 'lmm' else '')
    versions = {'R': first['R']['version'].replace('R version ', '').split(' (')[0],
                'statsmodels': first['statsmodels']['version'], 'NumPy': first['statsmodels'].get('numpy'),
                'Python': data.get('python'), 'Rust': (data.get('rust') or '').split(' (')[0].replace('rustc ', ''),
                'R BLAS': first['R'].get('BLAS')}
    if 'lme4' in first['R']['version']:
        versions['lme4'] = first['R']['version'].split('lme4 ')[1]
    return {
        'title': f'{title}: fit time', 'label': title,
        'description': f'Fit the same simulated data and return every result: phit {phit}, R {r}, and {py}. All three '
                       'read the same CSV and agree within the tolerance before timings are kept.',
        'workload': '1,000, 100,000, and 1,000,000 rows with 3 coefficients; 100,000 rows with 11 (counts include the '
                    'intercept)' + ('; min(100, n/20) groups' if slug == 'lmm' else ''),
        'dataset': {'seed': first and data['cases'][0]['data'].get('seed'),
                    'sha256': {f"n={c['n']},p={c['p']}": c['data']['sha256'] for c in data['cases']}},
        'implementations': {'phit': f'phit {phit}, PHP {first["PHP"]["version"]}', 'R': r, 'statsmodels': py},
        'measure': 'Elapsed time', 'unit': 'ms', 'statistic': 'median',
        'method': 'In-process wall-clock time of one complete fit with every output materialized, after one untimed '
                  'warm-up; five timed fits per worker. Process startup, CSV parsing, data generation, and JSON '
                  'serialization are excluded. Workers run in separate processes and alternate order between cases.',
        'time_spent_in': 'the measured process',
        'memory_measure': 'peak anonymous memory', 'memory_method': MEMORY_METHOD,
        'check': check,
        'environment': bench_environment(data, first['PHP']['version'], versions),
        'rows': rows,
        'reproduce': ['cargo build -p phit --release --locked',
                      f'PHP=$(command -v php) .context/bench-venv/bin/python tests/benchmarks/run.py --model {slug}'],
        'source': f'docs/benchmarks/{slug}-results.json', 'group': 'Models',
    }


def logistic_mixed_benchmark():
    data = load('docs/benchmarks/logistic-mixed-results.json')
    first = data['cases'][0]['implementations']
    rows, check = [], {}
    for case in data['cases']:
        label = f"n = {case['n']:,}, {case['n_groups']} groups"
        for name in ['PHP', 'SciPy reference', 'R']:
            result = case['implementations'][name]
            rows.append(timing_row(label, LABELS[name], result['fit_seconds'], result, r_timer=name == 'R'))
        for name, errors in case['max_absolute_errors_vs_R'].items():
            key = f"max_abs_difference_vs_R_{LABELS[name]}"
            check[key] = max(check.get(key, 0), max(errors.values()))
    t = data['tolerances']
    check['tolerance'] = (f"estimates {t['estimates']['absolute']:g} + {t['estimates']['relative']:g}·|R|; inference "
                          f"{t['inference']['absolute']:g} + {t['inference']['relative']:g}·|R|; likelihood "
                          f"{t['likelihood']['absolute']:g} + {t['likelihood']['relative']:g}·|R|; group modes "
                          f"{t['random_effects_absolute']:g} absolute (SciPy as a peer {t['peer_random_effects_absolute']:g})")
    return {
        'title': 'Binomial random intercept: fit time', 'label': 'Binomial mixed',
        'description': 'Fit a logit model with one random intercept by the Laplace approximation and return every '
                       'result: phit fit_mixed_logistic_matrix(), R lme4 glmer(), and an independent SciPy optimizer '
                       'of the same likelihood.',
        'workload': '1,000, 10,000, and 100,000 rows; 50 groups; 3 fixed coefficients',
        'dataset': {'seed': data['cases'][0].get('seed'), 'sha256': {f"n={c['n']}": c['data_sha256'] for c in data['cases']}},
        'implementations': {'phit': f'phit fit_mixed_logistic_matrix(), PHP {first["PHP"]["version"]}',
                            'R': 'lme4 glmer(), bobyqa', 'SciPy': 'SciPy L-BFGS-B on the same Laplace likelihood'},
        'measure': 'Elapsed time', 'unit': 'ms', 'statistic': 'median',
        'method': 'In-process wall-clock time of one complete fit with inference and observation-length outputs, after '
                  'one warm-up; three timed fits per worker. Startup, CSV reading, and JSON output are excluded.',
        'time_spent_in': 'the measured process',
        'memory_measure': 'peak anonymous memory', 'memory_method': MEMORY_METHOD, 'check': check,
        'environment': bench_environment(data, first['PHP']['version'], {
            'R': first['R']['version'].replace('R version ', '').split(' (')[0],
            'lme4': first['R']['version'].split('lme4 ')[-1], 'SciPy': first['SciPy reference']['version'].replace('SciPy ', ''),
            'Rust': (data.get('rust') or '').split(' (')[0].replace('rustc ', '')}),
        'rows': rows,
        'reproduce': ['cargo build -p phit --release --locked',
                      'PHP=$(command -v php) .context/bench-venv/bin/python tests/benchmarks/run-logistic-mixed.py'],
        'source': 'docs/benchmarks/logistic-mixed-results.json', 'group': 'Models',
    }


def distributions_benchmark():
    data = load('docs/benchmarks/distributions-results.json')
    rows = []
    php = None
    for case in data['cases']:
        label = f"{case['operation']}, n = {case['n']:,}"
        for name, result in case['implementations'].items():
            php = php or (result.get('version') if name.startswith('PHP') else None)
            rows.append(timing_row(label, LABELS.get(name, name), result['seconds'], result, digits=4,
                                   r_timer=False))
    impl = data['cases'][0]['implementations']
    return {
        'title': 'Distributions: evaluation time', 'label': 'Distributions',
        'description': 'Evaluate one distribution function over a batch of inputs: phit with one call per value and '
                       'with one batch call, the Rust core, R, and SciPy. Outputs are compared before timing.',
        'workload': '9 operations at 1,000, 100,000, and 1,000,000 inputs',
        'implementations': {'phit (scalar calls)': 'one phit call per value', 'phit (one batch call)': 'the _array form',
                            'Rust core': 'the stats-core function in a Rust worker', 'R': 'base R, vectorized',
                            'SciPy': 'scipy.stats, vectorized'},
        'measure': 'Elapsed time', 'unit': 'ms', 'statistic': 'median',
        'method': data.get('timing') + ' Small cases batch calls to stay above timer resolution; per-call times '
                                       'divide by the batch count.',
        'time_spent_in': 'the measured process',
        'memory_measure': 'peak anonymous memory', 'memory_method': MEMORY_METHOD,
        'check': {'agreement': data.get('verification')},
        'environment': bench_environment(data, php, {'R': impl['R']['version'].replace('R version ', '').split(' (')[0],
                                                       'SciPy': data.get('scipy'), 'NumPy': data.get('numpy'),
                                                       'Python': data.get('python'),
                                                       'Rust': (data.get('rust') or '').split(' (')[0].replace('rustc ', '')}),
        'rows': rows,
        'reproduce': ['cargo build -p phit --release --locked',
                      'cargo build -p stats-core --release --locked --example distribution_bench',
                      'PHP=$(command -v php) .context/bench-venv/bin/python tests/benchmarks/run-distributions.py'],
        'source': 'docs/benchmarks/distributions-results.json', 'group': 'Numerics',
    }


def draws_benchmark():
    data = load('docs/benchmarks/draws-results.json')
    rows = []
    first = data['cases'][0]['implementations']
    for case in data['cases']:
        for name, result in case['implementations'].items():
            for operation, reps in result['seconds'].items():
                row = {'case': f"n = {case['n']:,} draws, p = {case['p']}, {operation.replace('_', ' ')}",
                       'implementation': LABELS.get(name, name),
                       'value': round(statistics.median(reps) * 1000, 4), 'reps': [round(r * 1000, 4) for r in reps]}
                rows.append(row)
    return {
        'title': 'Posterior draws: moments and transforms', 'label': 'Posterior draws',
        'description': 'Weighted draw moments (population and unbiased), covariance maps, and affine transforms in '
                       'phit and R, after every output value is compared.',
        'workload': '1,000 × 4, 4,000 × 16, 10,000 × 32, and 100,000 × 8 draws × parameters',
        'implementations': {'phit': f'phit draws_* functions, PHP {first["PHP"]["version"]}',
                            'R': 'colSums, crossprod, chol, backsolve, and determinant on numeric matrices'},
        'measure': 'Elapsed time', 'unit': 'ms', 'statistic': 'median',
        'method': data.get('boundaries'),
        'time_spent_in': 'the measured process',
        'check': {'tolerance': data.get('tolerance'),
                  'max_abs_difference': max(a['max_absolute'] for c in data['cases'] for a in c['agreement'].values())},
        'environment': bench_environment(data, first['PHP']['version'], {
            'R': first['R']['version'].replace('R version ', '').split(' (')[0], 'jsonlite': first['R'].get('jsonlite'),
            'R BLAS': first['R'].get('blas'), 'Rust': (data.get('rust') or '').split(' (')[0].replace('rustc ', '')}),
        'rows': rows,
        'reproduce': ['cargo build -p phit --release --locked',
                      'PHP=$(command -v php) .context/bench-venv/bin/python tests/benchmarks/run-draws.py'],
        'source': 'docs/benchmarks/draws-results.json', 'group': 'Numerics',
    }


def rng_benchmark():
    data = load('docs/benchmarks/rng-results.json')
    rows = []
    php = data['cases'][0]['timing']['version']
    for case in data['cases']:
        result = case['timing']
        rows.append(timing_row(f"{case['family']}, n = {case['n']:,}", 'phit', result['seconds'], result, digits=4))
    return {
        'title': 'Seeded sampling: draw time', 'label': 'Seeded sampling',
        'description': 'One seeded call that returns n normal, beta, or Poisson draws. Sample points are checked '
                       'against independent ChaCha streams and R quantiles before timing.',
        'workload': 'normal, beta, and Poisson at 1,000, 100,000, and 1,000,000 draws, seed 42',
        'implementations': {'phit': f'phit dist_*_rng(), PHP {php}'},
        'measure': 'Elapsed time', 'unit': 'ms', 'statistic': 'median', 'method': data.get('timing'),
        'time_spent_in': 'the measured process',
        'memory_measure': 'peak anonymous memory', 'memory_method': MEMORY_METHOD,
        'check': {'verification': data.get('verification')},
        'environment': bench_environment(data, php, {'R (reference quantiles)': str(data.get('reference', '')).replace('R version ', '').split(' (')[0] or None,
                                                     'jsonlite': data.get('jsonlite')}),
        'rows': rows,
        'reproduce': ['cargo build -p phit --release --locked', 'PHP=$(command -v php) python3 tests/benchmarks/run-rng.py'],
        'source': 'docs/benchmarks/rng-results.json', 'group': 'Numerics',
    }


def csv_benchmark():
    data = load('docs/benchmarks/csv-results.json')
    rows = []
    for name, result in data['implementations'].items():
        rows.append(timing_row('1,000,000 rows, types guessed', name, result['seconds'], result, digits=1))
    php = next(iter(data['implementations'].values()))['php']
    return {
        'title': 'CSV reading: phit and phibble', 'label': 'CSV reading',
        'description': "Read one CSV file into PHP values with guessed types: Phit\\read_csv(), phibble's "
                       'DataFrame::fromCsv() using phit, and phibble with its own PHP reader.',
        'workload': f"{data['data']['rows']:,} rows, {data['data']['bytes'] / 1e6:.0f} MB: {data['data']['columns']}",
        'dataset': data['data'],
        'implementations': {'Phit\\read_csv()': f"phit {data['phit']} extension",
                            'phibble fromCsv(), extension': f"phibble {data['phibble']} with phit {data['phit']}",
                            'phibble fromCsv(), PHP reader': f"phibble {data['phibble']} without the extension"},
        'measure': 'Elapsed time', 'unit': 'ms', 'statistic': data.get('statistic', 'median'),
        'method': data.get('timing') + ' Each reader runs in its own process.',
        'time_spent_in': 'the measured process',
        'memory_measure': 'peak anonymous memory', 'memory_method': MEMORY_METHOD,
        'check': {'rows_read': data['data']['rows'],
                  'reader_used': 'each run reports whether Phit\\read_csv() was available, and the runner asserts it'},
        'environment': bench_environment(data, php, {'phibble': data['phibble'], 'phit': data['phit']}),
        'rows': rows,
        'reproduce': ['cargo build -p phit --release --locked',
                      'PHP=$(command -v php) python3 tests/benchmarks/run-csv.py --phibble /path/to/phibble'],
        'source': 'docs/benchmarks/csv-results.json', 'group': 'Input',
    }
