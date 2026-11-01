"""Shared-input binomial GLMM scale/benchmark: same Laplace likelihood in all workers."""
import argparse,hashlib,json,os,platform,statistics,subprocess,sys,tempfile
from datetime import datetime,timezone
from pathlib import Path
import numpy as np
from scipy.special import expit
from run import worker,THREAD_ENV
from machine import extension as machine_extension,machine
ROOT=Path(__file__).resolve().parents[2];HERE=Path(__file__).resolve().parent
parser=argparse.ArgumentParser();parser.add_argument('--sizes',default='1000,10000,100000');parser.add_argument('--repeats',type=int,default=3);args=parser.parse_args()
if args.repeats<1:parser.error('repeats must be positive')
php=os.environ.get('PHP','php');ext=machine_extension()
env={**os.environ,**THREAD_ENV};report=dict(recorded_at=datetime.now(timezone.utc).isoformat(),**machine(),
 model='binomial logit, one random intercept, Laplace',repeats=args.repeats,warmups=1,thread_environment=THREAD_ENV,
 extension_sha256=hashlib.sha256(ext.read_bytes()).hexdigest(),python_reference='Independent SciPy likelihood/optimizer; not BinomialBayesMixedGLM',
 tolerances=dict(estimates=dict(absolute=2e-5,relative=2e-4),inference=dict(absolute=2e-5,relative=2e-3),likelihood=dict(absolute=1e-6,relative=1e-8)),R_controls=dict(optimizer='bobyqa',tolPwrss=1e-12,initial_calc_derivs=False,restart_calc_derivs=True,restart_rhobeg=.001,rhoend=1e-9,restart_nAGQ0initStep=False),cases=[])
report['tolerances']['random_effects_absolute']=5e-5
report['rust']=subprocess.check_output(['rustc','--version'],text=True).strip()
with tempfile.TemporaryDirectory(dir=ROOT/'.context',prefix='logistic-mixed-benchmark-') as tmp:
 for n in map(int,args.sizes.split(',')):
  if n<100:parser.error('sizes must be >=100')
  rng=np.random.default_rng(20261011);G=50;g=np.arange(n)%G;study=rng.uniform(1,12,n);practice=.6*study+rng.normal(0,2,n)
  x=np.column_stack((np.ones(n),study,practice));beta=np.array([-2.,.2,.1]);effects=rng.normal(0,1,G);y=rng.binomial(1,expit(x@beta+effects[g]))
  path=Path(tmp)/'data.csv';np.savetxt(path,np.column_stack((y,g,x)),delimiter=',',fmt='%.17g',header='y,group,x0,x1,x2',comments='')
  results={};commands={'R':[os.environ.get('RSCRIPT','Rscript'),str(HERE/'logistic-mixed.R')], 'SciPy reference':[sys.executable,str(HERE/'logistic-mixed.py')],
   'PHP':[php,'-n','-d','memory_limit=-1','-d',f'extension={ext}',str(HERE/'logistic-mixed.php')]}
  for label,command in commands.items():
   print(n,label,flush=True);results[label]=worker([*command,str(path),str(args.repeats)],env)
   print('  median fit:',results[label]['median_fit_seconds'],flush=True)
  errors={}
  for label in ['SciPy reference','PHP']:
   assert results[label]['sample_indices']==results['R']['sample_indices'];errors[label]={}
   for key,value in results['R']['result'].items():
    if isinstance(value,bool):assert results[label]['result'][key]==value;continue
    policy=report['tolerances']['likelihood' if key=='log_likelihood' else ('inference' if key in ['covariance','std_errors'] else 'estimates')]
    a=results[label]['result'][key];absolute=5e-5 if key=='random_effects' else policy['absolute'];np.testing.assert_allclose(a,value,atol=absolute,rtol=policy['relative'],err_msg=f'{n}/{label}/{key}')
    errors[label][key]=float(np.max(np.abs(np.asarray(a)-value)))
  report['cases'].append(dict(n=n,p=3,n_groups=G,seed=20261011,data_sha256=hashlib.sha256(path.read_bytes()).hexdigest(),true_coefficients=beta.tolist(),
   true_random_intercept_sd=1.,sample_correlations=np.corrcoef([study,practice,y]).tolist(),correlation_order=["study_hours","practice_score","outcome"],implementations=results,max_absolute_errors_vs_R=errors,agreement_passed=True))
(ROOT/'docs/benchmarks/logistic-mixed-results.json').write_text(json.dumps(report,indent=2)+'\n')
print('All scale comparisons passed; benchmark report saved.')
