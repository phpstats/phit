#!/usr/bin/env python3
"""Time reading one CSV file with Phit\\read_csv() and with phibble's DataFrame::fromCsv(), with and without the
extension. Each implementation runs in its own process: one warm-up read, then the timed reads.

Data: phibble's bench/generate.php events.csv (id, key, g, x with ~2% NA, t datetime).
Run: PHP=$(command -v php) python3 tests/benchmarks/run-csv.py --phibble ~/src/phibble
     (phibble checked out with composer install; on Linux, peakmem on the PATH)
"""
import argparse
import hashlib
import json
import os
import statistics
import subprocess
import time
from datetime import datetime, timezone
from pathlib import Path

from machine import extension, machine, memory_command, peak_memory

ROOT = Path(__file__).resolve().parents[2]


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--phibble', type=Path, required=True, help='phibble checkout with vendor/')
    parser.add_argument('--rows', type=int, default=1_000_000)
    parser.add_argument('--repeats', type=int, default=5)
    parser.add_argument('--output', type=Path, default=ROOT / 'docs/benchmarks/csv-results.json')
    args = parser.parse_args()
    php = os.environ.get('PHP', 'php')
    ext = extension()
    phibble = args.phibble.expanduser().resolve()
    subprocess.run([php, str(phibble / 'bench/generate.php'), str(args.rows)], check=True)
    path = phibble / 'bench/data/events.csv'
    env = {**os.environ, 'PHIBBLE_AUTOLOAD': str(phibble / 'vendor/autoload.php')}
    script = str(ROOT / 'tests/benchmarks/csv.php')
    base = [php, '-n', '-d', 'memory_limit=-1']
    # Without php.ini, a shared mbstring (as on Ubuntu) is not loaded; phibble needs it in every run.
    if subprocess.run([*base, '-r', 'echo extension_loaded("mbstring") ? 1 : 0;'], capture_output=True,
                      text=True).stdout.strip() != '1':
        base += ['-d', 'extension=mbstring']
    runs = {
        'Phit\\read_csv()': [*base, '-d', f'extension={ext}', script, 'phit'],
        'phibble fromCsv(), extension': [*base, '-d', f'extension={ext}', script, 'phibble'],
        'phibble fromCsv(), PHP reader': [*base, script, 'phibble'],
    }
    results = {}
    for label, command in runs.items():
        start = time.perf_counter()
        done = subprocess.run([*memory_command(), *command, str(path), str(args.repeats)], env=env,
                              capture_output=True, text=True, check=True, timeout=3600)
        result = json.loads(done.stdout)
        result.update(peak_memory(done.stderr), process_seconds=time.perf_counter() - start,
                      median_seconds=statistics.median(result['seconds']))
        assert result['rows'] == args.rows, (label, result['rows'])
        results[label] = result
        print(f"{label}: {result['median_seconds']:.3f} s", flush=True)
    assert results['phibble fromCsv(), extension']['native'] and not results['phibble fromCsv(), PHP reader']['native']
    phibble_version = subprocess.run(['git', '-C', str(phibble), 'describe', '--tags', '--always'],
                                     capture_output=True, text=True).stdout.strip()
    report = dict(recorded_at=datetime.now(timezone.utc).isoformat(), **machine(),
                  phit=subprocess.check_output([*base, '-d', f'extension={ext}', '-r', 'echo phpversion("phit");'],
                                               text=True),
                  phibble=phibble_version, repeats=args.repeats, warmups=1, statistic='median',
                  data=dict(rows=args.rows, bytes=path.stat().st_size,
                            sha256=hashlib.sha256(path.read_bytes()).hexdigest(),
                            columns='id int, key int, g string, x float with ~2% NA, t datetime'),
                  timing='One read of the whole file into PHP values, types guessed; process startup excluded. '
                         'Memory covers the whole process.',
                  implementations=results)
    args.output.write_text(json.dumps(report, indent=2) + '\n')
    print(f'Wrote {args.output}')


if __name__ == '__main__':
    main()
