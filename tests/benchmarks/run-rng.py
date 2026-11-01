#!/usr/bin/env python3
"""Seeded PHP RNG: independent Python/R sample points and moments before timings.

Python stdlib + R/jsonlite only; the extension runtime has no such dependency.
"""
from datetime import datetime, timezone
import hashlib
import importlib.util
import json
import math
import os
from pathlib import Path
import platform
import re
import statistics
import subprocess
import tempfile
import time

from machine import extension as machine_extension, machine, memory_command, peak_memory

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('reference',ROOT/'tests/generate-rng-fixtures.py')
reference = importlib.util.module_from_spec(spec);spec.loader.exec_module(reference)


def probabilities(seed, indices):
    key = reference.seed_key(seed); cached = {}; out = []
    for i in indices:
        counter, offset = divmod(i,8)
        if counter not in cached:
            cached[counter] = reference.block(key,counter)
        words = cached[counter]
        word = words[2*offset] | (words[2*offset+1] << 32)
        out.append(((word >> 12)+.5)*2**-52)
    return out


def validate(result, case):
    assert result['n'] == case['n']
    for a,b in zip(result['sample'],case['expected']):
        assert math.isfinite(a) and abs(a-b) <= 1e-15+1e-12*abs(b), (a,b)
    # Independent theoretical checks, eight standard errors; fixed seeds make
    # these deterministic. They consume the complete output, outside timing.
    n = case['n']; family = case['family']
    mean, variance, fourth = dict(normal=(0.,1.,3.), beta=(2/7,10/392,(3-0.12)*(10/392)**2),
                                  poisson=(12.,12.,12.+3*12.**2))[family]
    assert abs(result['mean']-mean) <= 8*math.sqrt(variance/n)
    assert abs(result['variance']-variance) <= 8*math.sqrt((fourth-variance**2)/n)


def main():
    php = os.environ.get('PHP','php')
    extension = machine_extension()
    report = dict(recorded_at=datetime.now(timezone.utc).isoformat(),**machine(),
        extension_sha256=hashlib.sha256(extension.read_bytes()).hexdigest(),seed=42,repeats=5,warmups=1,
        timing='Complete seeded batch call, including ChaCha initialization, inverse CDF and PHP result allocation; summaries/JSON excluded. Peak RSS covers the entire process.',
        verification='17 independent Python ChaCha/PCG/Open01 points mapped by R quantiles at 1e-15 + 1e-12 relative, plus full-vector mean/variance within eight theoretical standard errors.',cases=[])
    def worker(path,mode):
        command=[php,'-n','-d','memory_limit=-1','-d',f'extension={extension}',str(ROOT/'tests/benchmarks/rng.php'),str(path),mode,'5']
        command=[*memory_command(),*command]
        start=time.perf_counter();done=subprocess.run(command,capture_output=True,text=True,check=True)
        result=json.loads(done.stdout);result['process_seconds']=time.perf_counter()-start
        result.update(peak_memory(done.stderr))
        return result
    with tempfile.TemporaryDirectory(prefix='rng-bench-',dir=ROOT/'.context') as directory:
        directory=Path(directory);r_cases=[];paths=[]
        for family,params in [('normal',dict(mean=0,sd=1)),('beta',dict(alpha=2,beta=5)),('poisson',{'lambda':12})]:
            for n in [1000,100000,1000000]:
                indices=[i*(n-1)//16 for i in range(17)]
                case=dict(family=family,params=params,n=n,seed=42,indices=indices)
                report['cases'].append(case)
                r_cases.append(dict(distribution=family,params=params,input=probabilities(42,indices)))
        source=directory/'uniforms.json';source.write_text(json.dumps(dict(cases=r_cases)))
        output=directory/'expected.json'
        subprocess.run([os.environ.get('RSCRIPT','Rscript'),str(ROOT/'tests/generate-rng-fixtures.R'),str(source),str(output)],check=True)
        refs=json.loads(output.read_text());report['reference']=refs['reference'];report['jsonlite']=refs['jsonlite']
        for i,case in enumerate(report['cases']):
            case['expected']=refs['expected'][i]
            path=directory/f'{i}.json';path.write_text(json.dumps(case));paths.append(path)
            case['input_sha256']=hashlib.sha256(path.read_bytes()).hexdigest()
            result=worker(path,'verify');validate(result['result'],case)
            case['verification']=result;case['agreement_passed']=True
            print(f'{case["family"]} n={case["n"]}: independent sample points and full-vector moments passed',flush=True)
        print('All numerical verification passed. Starting seeded-call timings.',flush=True)
        for path,case in zip(paths,report['cases']):
            result=worker(path,'time');validate(result['result'],case)
            result['median_seconds']=statistics.median(result['seconds']);case['timing']=result
            print(f'{case["family"]} n={case["n"]}: {result["median_seconds"]:.6g} seconds',flush=True)
    (ROOT/'docs/benchmarks/rng-results.json').write_text(json.dumps(report,indent=2)+'\n')


if __name__=='__main__':main()
