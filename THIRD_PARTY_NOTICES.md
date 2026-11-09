# Third-party notices

phit is released under the MIT license ([LICENSE](LICENSE)). The software below
is compiled into the extension or is followed by its code. Each keeps its own
license. The full texts are in [LICENSES/](LICENSES/). Release binaries also
carry `THIRD_PARTY_NOTICES.txt`, generated from the Cargo dependency closure by
`scripts/package-binary.py`.

## Compiled dependencies

### bobyqa 0.3.2

- <https://github.com/pawlenartowicz/bobyqa>
- MIT or Apache-2.0; phit uses it under MIT.
- Copyright (c) 2026 Paweł Lenartowicz
- License text: [LICENSES/MIT-bobyqa.txt](LICENSES/MIT-bobyqa.txt)
- Used by: the BOBYQA optimizer in `crates/stats-core/src/lmm.rs` and
  `crates/stats-core/src/logistic_mixed.rs`.

The crate implements the BOBYQA routine of PRIMA, so the PRIMA license applies
to that code:

- PRIMA <https://github.com/libprima/prima>
- BSD-3-Clause
- Copyright (c) 2020--2026, Zaikun ZHANG ( https://www.zhangzk.net )
- License text: [LICENSES/BSD-3-Clause-PRIMA.txt](LICENSES/BSD-3-Clause-PRIMA.txt)

### argmin 0.11.0 and argmin-math 0.5.1

- <https://github.com/argmin-rs/argmin>
- MIT or Apache-2.0; phit uses them under MIT.
- The upstream license file has no copyright line. The authors in the crate
  manifests are Stefan Kroboth and contributors.
- License text: [LICENSES/MIT-argmin.txt](LICENSES/MIT-argmin.txt)
- Used by: the Nelder–Mead optimizer for `fit_mixed_gaussian()` in
  `crates/stats-core/src/lmm.rs`.

### cdflib 0.4.3

- <https://github.com/vigna/CDFLIB-rs>
- MIT. The crate is a translation of the public-domain Fortran CDFLIB by Brown,
  Lovato, and Russell (ACM TOMS 708 and 654).
- The upstream license file has no copyright line. The author in the crate
  manifest is Sebastiano Vigna.
- License text: [LICENSES/MIT-cdflib.txt](LICENSES/MIT-cdflib.txt)
- Used by: the distribution tail probabilities in
  `crates/stats-core/src/distributions/`.

### statrs 0.19.1

- <https://github.com/statrs-dev/statrs>
- MIT
- Copyright (c) 2016 Michael Ma
- License text: [LICENSES/MIT-statrs.txt](LICENSES/MIT-statrs.txt)
- Used by: special functions and distributions inside the estimators in
  `crates/stats-core/src/`.

### nalgebra 0.35.0

- <https://github.com/dimforge/nalgebra>
- Apache-2.0
- The author in the crate manifest is Sébastien Crozet. The license file has no
  separate copyright line.
- License text: [LICENSES/Apache-2.0-nalgebra.txt](LICENSES/Apache-2.0-nalgebra.txt)
- Used by: the matrix decompositions throughout `crates/stats-core/src/`.

### rand 0.9.5 and rand_chacha 0.9.0

- <https://github.com/rust-random/rand>
- MIT or Apache-2.0; phit uses them under MIT.
- Copyright 2018 Developers of the Rand project
- Copyright (c) 2014 The Rust Project Developers
- License texts: [LICENSES/MIT-rand.txt](LICENSES/MIT-rand.txt) and
  [LICENSES/MIT-rand_chacha.txt](LICENSES/MIT-rand_chacha.txt)
- Used by: the seeded samplers in `crates/stats-core/src/distributions/rng.rs`.

### memchr 2.8.3

- <https://github.com/BurntSushi/memchr>
- Unlicense or MIT; phit uses it under MIT.
- Copyright (c) 2015 Andrew Gallant
- License text: [LICENSES/MIT-memchr.txt](LICENSES/MIT-memchr.txt)
- Used by: the CSV reader in `crates/stats-core/src/csv.rs`.

### ext-php-rs 0.16.2

- <https://github.com/extphprs/ext-php-rs>
- MIT or Apache-2.0; phit uses it under MIT.
- Copyright (c) 2021 David Cole <david.cole1340@gmail.com> and all contributors
- License text: [LICENSES/MIT-ext-php-rs.txt](LICENSES/MIT-ext-php-rs.txt)
- Used by: the PHP bindings in `crates/php-extension/src/`.

## Behavior followed

### phibble

- <https://github.com/phpstats/phibble>
- MIT
- Copyright (c) 2026 Erik Westlund
- License text: [LICENSES/MIT-phibble.txt](LICENSES/MIT-phibble.txt)
- Followed by: `crates/stats-core/src/csv.rs`, which reproduces phibble's PHP
  CSV reader. That reader follows readr, which is MIT.

## Routines written from specifications

These routines are written from the specifications in `specs/cleanroom/`,
which cite published methods, public-domain code, and R's documentation and
output. They contain no third-party code.

- One-dimensional minimization and root finding, in
  `crates/stats-core/src/optimize.rs`: R. P. Brent, *Algorithms for
  Minimization without Derivatives* (1973), chapters 4 and 5, and netlib's
  public-domain `fmin.f` and `zeroin.f` (<https://www.netlib.org/go/>).
  Specification: `specs/cleanroom/brent.md`.
- Printed number formatting, p-values, and coefficient table cells, in
  `crates/stats-core/src/rformat.rs`: R's help for `format`, `formatC`,
  `format.pval`, `printCoefmat`, and `options`, and R's output on the inputs
  in `specs/cleanroom/data/`. Specification: `specs/cleanroom/format.md`.
- Linear binning, linear and constant interpolation, the standard normal
  density, and the type-7 sample quantile, in
  `crates/stats-core/src/numeric.rs`: M. P. Wand, "Fast computation of
  multivariate kernel estimators", *Journal of Computational and Graphical
  Statistics* 3 (1994); R. J. Hyndman and Y. Fan, "Sample quantiles in
  statistical packages", *The American Statistician* 50 (1996), definition 7;
  and R's help for `density`, `approx`, `dnorm`, and `quantile`.
  Specification: `specs/cleanroom/smoothing.md`.
- The Stirling error, the deviance term, the saddle-point Poisson and binomial
  log probabilities, and the series and continued fractions for the
  incomplete gamma and beta functions, in
  `crates/stats-core/src/distributions/saddle.rs`: C. Loader, "Fast and
  accurate computation of binomial probabilities" (2000); M. Abramowitz and
  I. A. Stegun, *Handbook of Mathematical Functions*, 6.5.29, 6.5.31, and
  26.5.8; and the modified Lentz method of W. J. Lentz, *Applied Optics* 15
  (1976), and I. J. Thompson and A. R. Barnett, *Journal of Computational
  Physics* 64 (1986). Specification: `specs/cleanroom/special.md`.

## Instrumental-variable behavior references

The 2SLS implementation was written from published mathematics and public API
output. No AER, ivreg, fixest, or OneSampleMR source was used or copied.
Sanderson and Windmeijer (2016), Journal of Econometrics 190, 212–221
(doi:10.1016/j.jeconom.2015.06.004), defines the conditional F correction.
ivreg and fixest public documentation and fixture outputs specify inference
conventions. The references and observed differences are in
`specs/cleanroom/iv.md`. These are behavior references and introduce no bundled
GPL code.
