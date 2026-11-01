"""Writes the two benchmark data sets to .context/bench-data as CSV.

    python3 bench/generate.py [--ols-rows N] [--mixed-rows N] [--mixed-groups G]
"""
import argparse
from pathlib import Path

import numpy as np

ROOT = Path(__file__).resolve().parents[1]
OUT = ROOT / '.context' / 'bench-data'


def ols(n, path):
    rng = np.random.default_rng(20261101)
    x = rng.normal(0.0, 1.0, (n, 10))
    beta = np.array([0.8, -0.5, 0.3, 1.2, 0.0, -0.9, 0.4, 0.25, -0.15, 0.6])
    y = 2.0 + x @ beta + rng.normal(0.0, 1.5, n)
    columns = {f'x{j + 1}': x[:, j] for j in range(10)} | {'y': y}
    write(path, columns)


def mixed(n, groups, path):
    rng = np.random.default_rng(20261102)
    g = rng.integers(0, groups, n)
    effect = rng.normal(0.0, 2.0, groups)
    x = rng.normal(0.0, 1.0, (n, 3))
    y = 10.0 + x @ np.array([1.5, -0.7, 0.3]) + effect[g] + rng.normal(0.0, 3.0, n)
    write(path, {'g': np.array([f'g{v:04d}' for v in g]), 'x1': x[:, 0], 'x2': x[:, 1], 'x3': x[:, 2], 'y': y})


def write(path, columns):
    names = list(columns)
    n = len(columns[names[0]])
    with open(path, 'w') as f:
        f.write(','.join(names) + '\n')
        cells = [[v if isinstance(v, str) else repr(float(v)) for v in columns[name]] for name in names]
        for i in range(n):
            f.write(','.join(c[i] for c in cells) + '\n')


if __name__ == '__main__':
    parser = argparse.ArgumentParser()
    parser.add_argument('--ols-rows', type=int, default=1_000_000)
    parser.add_argument('--mixed-rows', type=int, default=20_000)
    parser.add_argument('--mixed-groups', type=int, default=200)
    args = parser.parse_args()
    OUT.mkdir(parents=True, exist_ok=True)
    ols(args.ols_rows, OUT / 'ols.csv')
    mixed(args.mixed_rows, args.mixed_groups, OUT / 'mixed.csv')
    print(f'wrote {OUT}/ols.csv ({args.ols_rows} rows) and mixed.csv ({args.mixed_rows} rows, {args.mixed_groups} groups)')
