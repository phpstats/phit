"""Audit saved R references against actual PHP calls; no R needed unless --refresh."""
from pathlib import Path
import argparse, hashlib, json, os, platform, subprocess, warnings
ROOT = Path(__file__).resolve().parents[2]
PHP = os.environ.get('PHP', '/opt/homebrew/bin/php' if Path('/opt/homebrew/bin/php').exists() else 'php')
EXT = Path(os.environ.get('STATS_EXTENSION', str(ROOT/'target/release'/('libphit.dylib' if platform.system()=='Darwin' else 'libphit.so'))))

def fit_php(case):
    return json.loads(subprocess.run([PHP,'-n','-d',f'extension={EXT}',str(ROOT/'tests/compatibility-fit.php')],
        input=json.dumps(case),capture_output=True,text=True,check=True).stdout)

def check(actual, expected, absolute, relative, path=''):
    if isinstance(expected, dict):
        for key,value in expected.items():check(actual[key],value,absolute,relative,path+'/'+key)
    elif isinstance(expected, list):
        assert len(actual)==len(expected), path
        for i,value in enumerate(expected):check(actual[i],value,absolute,relative,path+'/'+str(i))
    elif isinstance(expected,bool) or path.endswith(("/df_residual","/n_parameters")):assert actual==expected,path
    else:assert abs(actual-expected)<=absolute+relative*abs(expected),(path,actual,expected)

def max_diff(actual, expected):
    if isinstance(expected, dict):return max(max_diff(actual[k],v) for k,v in expected.items())
    if isinstance(expected, list):return max((max_diff(a,b) for a,b in zip(actual,expected)),default=0)
    if isinstance(expected,bool):return int(actual!=expected)
    return abs(actual-expected)

def expanded_term(term, spec):
    """A PHP random term in the reference's shape: (x || g) keeps only conditional-variance diagonals, as lme4
    reports them (tests/lmm-expanded.php does the same)."""
    out=dict(covariance=term['covariance'],effects=term['effects'])
    if spec.get('correlated',True):out['effects_cond_var']=term['effects_cond_var']
    else:out['effects_cond_var_diagonal']=[[block[j][j] for j in range(len(block))] for block in term['effects_cond_var']]
    return out

def sleepstudy_python(case):
    import numpy as np
    import scipy.linalg as la
    import scipy.stats as st
    import statsmodels.api as sm
    x=np.array(case['x']);y=np.array(case['y']);t=case['terms'][0];z=np.array(t['z']);g=np.array(t['groups'])
    with warnings.catch_warnings(record=True) as caught:
        warnings.simplefilter('always')
        fitted=sm.MixedLM(y,x,groups=g,exog_re=z).fit(reml=case['reml'],method='bfgs',gtol=1e-8,maxiter=2000)
    assert fitted.converged,case['name']
    # Plug-in GLS covariance matches lme4 vcov; statsmodels' joint-Hessian bse differs.
    v=np.eye(len(y))*fitted.scale
    for group in dict.fromkeys(g):
        ids=np.flatnonzero(g==group);v[np.ix_(ids,ids)]+=z[ids]@fitted.cov_re@z[ids].T
    chol=la.cho_factor(v);cv=la.solve(x.T@la.cho_solve(chol,x),np.eye(x.shape[1]),assume_a='pos')
    beta=np.asarray(fitted.fe_params);se=np.sqrt(cv.diagonal());stat=beta/se;fixed=x@beta
    effects=np.array([np.asarray(fitted.random_effects[group]) for group in dict.fromkeys(g)])
    conditional=np.asarray(fitted.fittedvalues);df=len(y)-len(beta)-4;np_=len(beta)+4
    ll=float(fitted.llf)
    return dict(coefficients=beta.tolist(),covariance=cv.tolist(),std_errors=se.tolist(),z_statistics=stat.tolist(),
        p_values=(2*st.norm.sf(abs(stat))).tolist(),conf_int=np.column_stack((beta-st.norm.ppf(.975)*se,beta+st.norm.ppf(.975)*se)).tolist(),
        fixed_fitted=fixed.tolist(),fitted=conditional.tolist(),residuals=(y-conditional).tolist(),marginal_residuals=(y-fixed).tolist(),
        terms=[dict(covariance=np.asarray(fitted.cov_re).tolist(),effects=effects.tolist())],sigma=float(np.sqrt(fitted.scale)),
        residual_variance=float(fitted.scale),log_likelihood=ll,aic=2*np_-2*ll,bic=np_*np.log(len(y))-2*ll,
        df_residual=df,n_parameters=np_,singular=False),[str(w.message) for w in caught]

def run():
    fixed=json.loads((ROOT/'tests/fixtures/r-compatibility.json').read_text())
    mixed=json.loads((ROOT/'tests/fixtures/lmm-expanded-r.json').read_text())
    fixed_results=[];mixed_results=[]
    for case in fixed['cases']:
        fit=fit_php(case);check(fit,case['expected'],1e-7,1e-6,case['name'])
        fixed_results.append(dict(name=case['name'],model=case['model'],R=case['expected'],PHP=fit,
            max_absolute_error=max_diff(fit,case['expected'])))
    for case in mixed['cases']:
        native={}
        for optimizer in ['bobyqa','nelder_mead']:
            fit=fit_php(dict(case,optimizer=optimizer))
            fit['terms']=[expanded_term(t,term) for t,term in zip(fit['random'],case['terms'])]
            native[optimizer]=fit
        errors=[]
        for optimizer,reference in case['references'].items():
            assert reference['optimizer_code']==[0],(case['name'],optimizer,reference['messages'])
            assert not reference['messages'],(case['name'],optimizer,reference['messages'])
            for backend,fit in native.items():
                for key,value in reference['result'].items():
                    tol=(5e-6,1e-9) if key in ['log_likelihood','aic','bic'] else (
                        (1e-5,1e-5) if optimizer=='bobyqa' and backend=='bobyqa' else (1e-3,3e-4))
                    check(fit[key],value,*tol,path=f"{case['name']}/{backend}/{optimizer}/{key}")
                errors.append(dict(native_optimizer=backend,R_optimizer=optimizer,
                    log_likelihood_difference=native[backend]['log_likelihood']-reference['result']['log_likelihood'],
                    max_fitted_difference=max_diff(fit['fitted'],reference['result']['fitted']),
                    max_random_covariance_difference=max_diff([t['covariance'] for t in fit['terms']],
                                                              [t['covariance'] for t in reference['result']['terms']])))
        row=dict(name=case['name'],observations=len(case['y']),method='REML' if case['reml'] else 'ML',
            R=case['references'],PHP=native,comparisons=errors)
        if case['name'].startswith('sleepstudy_'):
            py,py_warnings=sleepstudy_python(case)
            for key,value in case['references']['bobyqa']['result'].items():
                tol=(5e-6,1e-9) if key in ['log_likelihood','aic','bic'] else (1e-3,3e-4)
                if key=='terms':
                    # statsmodels reports no conditional variances of the random effects.
                    value=[dict(covariance=v['covariance'],effects=v['effects']) for v in value]
                check(py[key],value,*tol,path=case['name']+'/statsmodels/'+key)
            row.update(statsmodels=py,statsmodels_warnings=py_warnings)
        mixed_results.append(row)
    import numpy, scipy, statsmodels
    paths=['tests/fixtures/r-compatibility.json','tests/fixtures/lmm-expanded-r.json','docs/data/lmm/sleepstudy.csv']
    report=dict(versions=dict(R=fixed['r'],lme4=mixed['lme4'],PHP=subprocess.check_output([PHP,'-n','-r','echo PHP_VERSION;'],text=True),
        python=platform.python_version(),numpy=numpy.__version__,scipy=scipy.__version__,statsmodels=statsmodels.__version__),
        sha256={p:hashlib.sha256((ROOT/p).read_bytes()).hexdigest() for p in paths},extension_sha256=hashlib.sha256(EXT.read_bytes()).hexdigest(),
        tolerances=dict(fixed=dict(absolute=1e-7,relative=1e-6),mixed_bobyqa=dict(absolute=1e-5,relative=1e-5),
        mixed_other_optimizers=dict(absolute=1e-3,relative=3e-4),mixed_likelihood=dict(absolute=5e-6,relative=1e-9)),
        fixed=fixed_results,mixed=mixed_results)
    target=ROOT/'docs/verifications/r-compatibility-results.json';target.write_text(json.dumps(report,indent=2)+'\n')
    print(f"Passed {len(fixed_results)} fixed-model cases and {len(mixed_results)} mixed-model cases × 2 native × 4 lme4 optimizers; sleepstudy ML/REML also passed statsmodels.")
    return report
if __name__=='__main__':
    parser=argparse.ArgumentParser();parser.add_argument('--refresh',action='store_true');args=parser.parse_args()
    if args.refresh:
        for script in ['tests/generate-r-compatibility-fixtures.R','tests/generate-lmm-expanded-fixtures.R']:
            subprocess.run(['Rscript',script],cwd=ROOT,check=True)
    run()
