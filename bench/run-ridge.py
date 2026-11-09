"""Ridge-only workload benchmarks; fresh processes, warm-up, rotated order, 7 runs."""
import datetime
import hashlib
import json
import os
from pathlib import Path
import platform
import statistics
import subprocess

ROOT = Path(__file__).resolve().parents[1]
PHP = os.environ.get('PHP', '/opt/homebrew/opt/php@8.4/bin/php')
EXT = os.environ.get('STATS_EXTENSION', str(ROOT / 'target/release/libphit.dylib'))
POLY = Path(os.environ.get('PHIT_POLYFILL', str(ROOT.parent / 'phit-polyfill')))
commands = {
 'phit': [PHP, '-n', '-d', f'extension={EXT}', str(ROOT / 'bench/ridge.php')],
 'phit-polyfill': [PHP, '-d', 'opcache.enable_cli=0', '-d', f'auto_prepend_file={POLY}/vendor/autoload.php', str(ROOT / 'bench/ridge.php')],
 'R glmnet': ['Rscript', str(ROOT / 'bench/ridge.R')],
}
env = dict(os.environ)
for name in ['OPENBLAS_NUM_THREADS','OMP_NUM_THREADS','MKL_NUM_THREADS','VECLIB_MAXIMUM_THREADS','BLIS_NUM_THREADS']: env[name] = '1'
rows = []
worst = 0.0
for case in ['standardized', 'heating', 'heating_1152', 'cv_heating']:
 samples = {key: [] for key in commands}
 for round in range(-1,7):
  names = list(commands)
  names = names[round % len(names):] + names[:round % len(names)]
  reference = None
  for name in names:
   result = json.loads(subprocess.check_output([*commands[name],case],cwd=ROOT,env=env,text=True))
   if reference is None: reference = result['coefficients']
   difference = max(abs(a-b) for a,b in zip(result['coefficients'],reference))
   worst = max(worst,difference)
   assert all(abs(a-b)<=1e-7+2e-6*abs(b) for a,b in zip(result['coefficients'],reference)), (case,name,difference)
   if round >= 0: samples[name].append(result['ms'])
  if round == 6:
   for name, values in samples.items(): rows.append({'case':case,'implementation':name,'value':statistics.median(values),'reps':values,'n':result['n'],'k':result['k']})
 print(f'Ridge benchmark complete: {case}',flush=True)
record = {'title':'Gaussian ridge and CV workloads','label':'Ridge','measure':'Elapsed time','unit':'ms','statistic':'median',
 'environment': {'os':platform.platform(),'cpu':platform.processor(),'threads':'One thread; JIT and CLI OPcache disabled','repeats':7,'date':str(datetime.date.today()),
                 'php':subprocess.check_output([PHP,'-r','echo PHP_VERSION;'],text=True), 'reference':json.loads((ROOT/'tests/fixtures/ridge-r.json').read_text())['reference']},
 'code_sha256': {str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else 'phit-polyfill/src/Ridge.php': hashlib.sha256(path.read_bytes()).hexdigest() for path in [ROOT/'bench/ridge.php',ROOT/'bench/ridge.R',ROOT/'bench/run-ridge.py',ROOT/'crates/stats-core/src/ridge.rs',POLY/'src/Ridge.php']},
 'rows':rows,'check':{'max_abs_coefficient_or_cv_difference':worst},
 'notes':['Fresh process per run; one excluded warm-up per implementation and workload; rotated execution order.',
          'Data parsing and process/package startup excluded. Formula construction, estimation, covariance (phit), and fold refits (CV) included.',
          'phit and polyfill compute conditional covariance and effective df for every fit. R glmnet does not compute covariance and performs less work.',
          'The 1152-row heating case repeats the 288-row saved input four times. Only ridge workloads are run. CV uses five lambdas and day folds with age-decaying weights.'],
 'reproduce':['python3 bench/run-ridge.py'],'source':'bench/ridge-results.json'}
(ROOT/'bench/ridge-results.json').write_text(json.dumps(record,indent=2)+'\n')
