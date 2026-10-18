#!/usr/bin/env python3
"""Simulate site/data/rents.csv for the rent-model vignette (fixed seed; no dependencies).

monthly_rent = 450 + 17.5 * area_m2 + 110 * bedrooms + Normal(0, 160), rounded to cents.
"""
import csv
import random
from pathlib import Path

rng = random.Random(20261017)
rows = []
for _ in range(400):
    area = round(rng.uniform(28, 125), 1)
    bedrooms = max(1, min(4, round(area / 32 + rng.gauss(0, 0.6))))
    rent = 450 + 17.5 * area + 110 * bedrooms + rng.gauss(0, 160)
    rows.append((area, bedrooms, round(rent, 2)))
with open(Path(__file__).with_name('rents.csv'), 'w', newline='') as f:
    writer = csv.writer(f)
    writer.writerow(['area_m2', 'bedrooms', 'monthly_rent'])
    writer.writerows(rows)
