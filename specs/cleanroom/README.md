# Clean-room specifications

These specifications describe routines that phit's core needs, written so that they can be implemented without R's
source code. A provenance review found that the earlier versions of these routines followed R's GPL-licensed source.
phit is MIT-licensed, so each routine is reimplemented from these documents alone.

The rules below govern the listed routine replacements. New IV integration is
specified in [iv.md](iv.md), which permits reading existing MIT phit interfaces
and excludes reference-package source.

## Rules for the implementer

- Work only from the documents in this directory, the data files in `data/`, the published works cited in each
  specification, and public-domain code they name (for example netlib's `fmin.f` and `zeroin.f`).
- Do not open, search for, or recall R's C or R source code, lme4's source, or anything under `crates/` or `tests/` in
  the phit repository. Do not read GNU Scientific Library or Numerical Recipes code.
- Use your own structure and names. The signatures in each specification are the interface phit calls and must be kept.
- Check your implementation against the tables in `data/` and report the largest difference you see for each table.

## Sources of the specifications

The specifications were written from:
- the published algorithms cited in each one, and the public-domain netlib sources;
- R's user documentation (the help pages for `format`, `formatC`, `format.pval`, `printCoefmat`, `approx`,
  `quantile`, `dnorm`, and `density`);
- R's observed output on the inputs in `data/`, recorded by `generate-r-tables.R`;
- high-precision reference values from mpmath, recorded by `generate-mpmath-tables.py`.

The author of the specifications had read R's source code before writing them. The implementer must not have. That
separation is the reason for this process.

| Specification | Routines | Data |
| --- | --- | --- |
| [brent.md](brent.md) | One-dimensional minimization and root finding | `../../tests/fixtures/optimize-r.json` (copied to `data/optimize.json`) |
| [format.md](format.md) | R's printed number format, p-values, and coefficient tables | `format.json`, `format_g.json`, `format_pval.json`, `round.json`, `coefmat.json` |
| [smoothing.md](smoothing.md) | Linear binning, interpolation, the normal density, and the type-7 quantile | `approx.json`, `dnorm.json`, `quantile7.json` |
| [special.md](special.md) | Stirling error, deviance term, Poisson and binomial log probabilities, and incomplete gamma and beta | `special.json` |
