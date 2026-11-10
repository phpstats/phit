"""Robust covariance workload benchmarks; fresh processes, warm-up, rotated order, 7 runs."""
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
 'phit': [PHP, '-n', '-d', f'extension={EXT}', str(ROOT / 'bench/vcov.php')],
 'phit-polyfill': [PHP, '-d', 'opcache.enable_cli=0', '-d', f'auto_prepend_file={POLY}/vendor/autoload.php', str(ROOT / 'bench/vcov.php')],
 'R sandwich': ['Rscript', str(ROOT / 'bench/vcov.R')],
}
env = dict(os.environ)
for name in ['OPENBLAS_NUM_THREADS','OMP_NUM_THREADS','MKL_NUM_THREADS','VECLIB_MAXIMUM_THREADS','BLIS_NUM_THREADS']: env[name] = '1'
rows = []
worst = 0.0
for case in ['heating_ols_1', 'heating_iv_1', 'heating_iv_2', 'heating_iv_4']:
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
   assert all(abs(a-b)<=1e-9+1e-7*abs(b) for a,b in zip(result['coefficients'],reference)), (case,name,difference)
   if round >= 0: samples[name].append(result['ms'])
  if round == 6:
   for name, values in samples.items(): rows.append({'case':case,'implementation':name,'value':statistics.median(values),'reps':values,'n':result['n'],'k':result['k']})
 print(f'Robust covariance benchmark complete: {case}',flush=True)
record = {'title':'Robust covariance workloads','label':'Robust covariance','measure':'Elapsed time','unit':'ms','statistic':'median',
 'environment': {'os':platform.platform(),'cpu':platform.processor(),'threads':'One thread; JIT and CLI OPcache disabled','repeats':7,'date':str(datetime.date.today()),
                 'php':subprocess.check_output([PHP,'-r','echo PHP_VERSION;'],text=True), 'reference':json.loads((ROOT/'tests/fixtures/vcov-r.json').read_text())['reference']},
 'code_sha256': {str(path.relative_to(ROOT)) if path.is_relative_to(ROOT) else 'phit-polyfill/src/Vcov.php': hashlib.sha256(path.read_bytes()).hexdigest() for path in [ROOT/'bench/vcov.php',ROOT/'bench/vcov.R',ROOT/'bench/run-vcov.py',ROOT/'crates/stats-core/src/vcov.rs',POLY/'src/Vcov.php']},
 'rows':rows,'check':{'max_abs_estimate_or_covariance_difference':worst},
 'notes':['Fresh process per run; one excluded warm-up per implementation and workload; rotated execution order.',
          'Data parsing and process/package startup excluded. Formula construction, estimation, covariance, and diagnostics included.',
          'IV phit and polyfill include Shea and SW diagnostics. R ivreg includes its public summary diagnostics but has no SW or Shea output; it performs less work.',
          'Only robust covariance workloads are run: heating OLS HC3, IV HC3, IV day clusters, and IV Newey-West lag24 with adjustment.'],
 'reproduce':['python3 bench/run-vcov.py'],'source':'bench/vcov-results.json'}
(ROOT/'bench/vcov-results.json').write_text(json.dumps(record,indent=2)+'\n')
