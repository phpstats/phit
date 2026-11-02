# Summary fixtures

`summary.json` holds the cases for `Phit\summary()` and `Phit\tidy()`: each case's input columns, the model call,
the expected summary text, the expected tidy columns, and R's printed coefficient rows for the same model.
`tests/summary.php` checks phit against all three, and phit-polyfill copies the file to check byte-identical output.
`SPEC.md` gives the formatting rules, the layout, the keys read, and the errors.

Regenerate, from the repository root, with R (lme4) and a release build:

```sh
Rscript tests/generate-summary-r.R
php -n -d extension=target/release/libphit.dylib tests/generate-summary-fixtures.php
```

The data are copied unchanged from [phpstats/datasets](https://github.com/phpstats/datasets), which took them from
the R packages' Rdatasets exports. They remain under their packages' licenses; GPL version 2 is in
`docs/data/lmm/COPYING`.

| File | From | License |
| --- | --- | --- |
| `mtcars.csv` | R `datasets` (`mtcars`) | GPL-2 or GPL-3, as part of R |
| `mroz.csv` | R `carData` 3.0-6 (`Mroz`) | GPL (≥ 2) |
| `insurance.csv` | R `MASS` 7.3-65 (`Insurance`) | GPL-2 or GPL-3 |
| `docs/data/lmm/sleepstudy.csv` | R `lme4` (`sleepstudy`) | GPL (≥ 2) |
| `docs/data/cbpp.csv` | R `lme4` (`cbpp`) | GPL (≥ 2) |
