#!/usr/bin/env python3
"""Refresh/check static Quarto evidence tables from recorded JSON; stdlib only."""
import json
import math
from pathlib import Path
import sys

ROOT = Path(__file__).resolve().parents[1]


def read(path):
    return json.loads((ROOT/path).read_text())


def table(headers, rows):
    return '\n'.join(['| '+' | '.join(headers)+' |', '| '+' | '.join(['---']*len(headers))+' |',
                      *['| '+' | '.join(map(str,row))+' |' for row in rows]])


def replace(path, marker, content):
    path = ROOT/path
    start, end = f'<!-- BEGIN {marker} -->', f'<!-- END {marker} -->'
    old = path.read_text()
    before, rest = old.split(start, 1)
    _, after = rest.split(end, 1)
    new = before+start+'\n\n'+content+'\n\n'+end+after
    if '--check' in sys.argv:
        if old != new:
            raise SystemExit(f'{path.relative_to(ROOT)} tables drifted; run scripts/distribution-docs.py')
    else:
        path.write_text(new)


report = read('docs/verifications/distributions-results.json')
reference = read('tests/fixtures/distributions-mpmath.json')
worst = {}
for case, original in zip(report['cases'], reference['cases']):
    family = case['distribution']
    worst.setdefault(family, {name:0. for name in ('php','r','scipy')})
    for i, ref in enumerate(original['expected']):
        if not isinstance(ref, (float,int)) or ref == 0:
            continue
        if case['operation']=='quantile' and original['input'][i] in (0,1):
            continue
        for name in worst[family]:
            e = case['errors'][name][i]
            if isinstance(e, (float,int)) and math.isfinite(e):
                worst[family][name] = max(worst[family][name],e)
rows = [[family,*[f'{v:.3g}' for v in errors.values()]] for family,errors in sorted(worst.items())]
text = (f"Native refresh: **{report.get('native_refreshed_at',report['recorded_at'])}**. "
        f"PHP {report['php']}; {report['r']}; SciPy {report['scipy']}; mpmath {report['mpmath']} at {report['mpmath_digits']} digits.\n\n"
        f"{len(report['cases'])} cases, {sum(c['n'] for c in report['cases']):,} points, "
        f"{sum(c['references_unavailable'] for c in report['cases'])} unavailable independent reference points.\n\n"
        +table(['Family','Worst native error','Worst R error','Worst SciPy error'],rows))
replace('docs/verifications/distributions.qmd','REFERENCE EVIDENCE',text)

bench = read('docs/benchmarks/distributions-results.json')
if not bench.get('complete'):
    raise SystemExit('Distribution benchmark is incomplete; leave prior timing tables unchanged')
names = ['PHP_scalar','PHP_batch','Rust','R','SciPy']
rows = []
disagreements = []
for c in bench['cases']:
    assert c['agreement_passed']
    rows.append([c['operation'],f"{c['n']:,}",*[f"{1000*c['implementations'][n]['median_seconds']:.4g}" if n in c['implementations'] else '—' for n in names]])
    if not c['agreement']['SciPy']['agreement_passed']:
        disagreements.append(f"{c['operation']} ({c['n']:,})")
memory = max(r['peak_rss_bytes'] or 0 for c in bench['cases'] for r in c['implementations'].values())/2**20
text = (f"Recorded: **{bench['recorded_at']}**; {bench.get('cpu',bench['platform'])}, {bench['platform']}. "
        f"{len(bench['cases'])} input cases; all native/R comparisons passed. "
        f"SciPy exceeded the same tolerance for: {', '.join(disagreements) or 'none'}.\n\n"
        f"Median milliseconds per full call, {bench['repeats']} measurements after one warm-up:\n\n"
        +table(['Operation','Values','PHP scalar loop','PHP batch/native','Rust loop','R','SciPy'],rows)
        +f"\n\nWhole-process peak RSS across these workers reached {memory:.1f} MiB; "
         'per-worker memory, load/startup times and every timing sample are retained in the raw report.')
replace('docs/benchmarks/distributions.qmd','BENCHMARK EVIDENCE',text)

rng = read('docs/benchmarks/rng-results.json')
rows = []
for c in rng['cases']:
    assert c['agreement_passed']
    r = c['timing']
    rows.append([c['family'],f"{c['n']:,}",f"{r['median_seconds']*1000:.4g}",
                 f"{r['peak_rss_bytes']/2**20:.1f}" if r['peak_rss_bytes'] else 'unavailable'])
text = (f"Recorded: **{rng['recorded_at']}**; {rng['platform']}; PHP {rng['cases'][0]['timing']['version']}. "
        f"All {len(rng['cases'])} sample-point/moment comparisons passed before timings. "
        f"Seed {rng['seed']}, {rng['repeats']} repetitions.\n\n"
        +table(['Family','Draws','Median full call (ms)','Peak whole-process RSS (MiB)'],rows))
replace('docs/benchmarks/rng.qmd','RNG EVIDENCE',text)
print('Distribution reference and benchmark tables verified' if '--check' in sys.argv else 'Distribution evidence tables updated')
