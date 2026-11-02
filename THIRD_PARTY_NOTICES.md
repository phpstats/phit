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

## Routines being rewritten

A provenance review found that some routines follow R's source code. R is
GPL-2-or-later. These routines are being rewritten from public-domain sources
and published methods:

- `crates/stats-core/src/optimize.rs`
- the number formatting in `crates/stats-core/src/summary.rs`
- the binning, interpolation, and normal density in
  `crates/stats-core/src/smoothing.rs` and
  `crates/stats-core/src/distributions/continuous.rs`
- the saddle-point and continued-fraction helpers in
  `crates/stats-core/src/distributions/special.rs`
- the type-7 quantile in `crates/stats-core/src/descriptive.rs`

Once the rewrite lands, this section will list the source of each routine.
