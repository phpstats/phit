# One benchmark run in a fresh process: Rscript bench/fit.R {ols|mixed} {data.csv|--baseline}
# Prints one JSON line: elapsed ms, coefficients and standard errors. The runner measures memory.
args <- commandArgs(trailingOnly = TRUE)
model <- args[1]
path <- args[2]
if (model == "mixed") suppressPackageStartupMessages(library(lme4))
if (path == "--baseline") quit(save = "no")

# Untimed: read the CSV into a data.frame.
d <- read.csv(path, colClasses = c(g = "character"))

start <- Sys.time()
if (model == "ols") {
  fit <- lm(y ~ x1 + x2 + x3 + x4 + x5 + x6 + x7 + x8 + x9 + x10, data = d)
  table <- coef(summary(fit))
} else {
  fit <- lmer(y ~ x1 + x2 + x3 + (1 | g), data = d, REML = TRUE)
  table <- coef(summary(fit))
}
elapsed <- as.numeric(difftime(Sys.time(), start, units = "secs")) * 1000

json <- function(v) paste0("[", paste(sprintf("%.17g", v), collapse = ","), "]")
cat(sprintf('{"ms":%.6f,"names":[%s],"coefficients":%s,"std_errors":%s,"version":"%s","lme4":%s}\n',
  elapsed, paste0('"', rownames(table), '"', collapse = ","), json(table[, 1]), json(table[, 2]),
  R.version$version.string,
  if (model == "mixed") paste0('"', packageVersion("lme4"), '"') else "null"))
