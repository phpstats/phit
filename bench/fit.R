# One benchmark run in a fresh process: Rscript bench/fit.R WORKLOAD {data.csv|--baseline}
# Prints one JSON line: elapsed ms and the estimates. The runner measures memory, and records the lines between a
# workload's [name] and [/name] markers as its timed code.
args <- commandArgs(trailingOnly = TRUE)
workload <- args[1]
path <- args[2]
if (workload %in% c("mixed", "mixed-logistic")) suppressPackageStartupMessages(library(lme4))
if (path == "--baseline") quit(save = "no")

# Untimed: the data in a data.frame, or the input vectors.
if (workload == "distributions") {
  n <- 1e6
  x <- -5 + 10 * (0:(n - 1)) / (n - 1)
  p <- ((0:(n - 1)) + 0.5) / n
  positive <- 20 * ((0:(n - 1)) + 0.5) / n
} else {
  d <- read.csv(path)
  if ("g" %in% names(d)) d$g <- as.character(d$g)
}

start <- Sys.time()
if (workload == "ols") {
  # [ols]
  fit <- lm(
    y ~ x1 + x2 + x3 + x4 + x5 + x6 + x7 + x8 + x9 + x10,
    data = d
  )
  table <- coef(summary(fit))
  # [/ols]
} else if (workload == "mixed") {
  # [mixed]
  fit <- lmer(
    y ~ x1 + x2 + x3 + (1 | g),
    data = d,
    REML = TRUE
  )
  table <- coef(summary(fit))
  # [/mixed]
} else if (workload == "logistic") {
  # [logistic]
  fit <- glm(
    y ~ x1 + x2 + x3 + x4 + x5 + x6 + x7 + x8 + x9 + x10,
    data = d,
    family = binomial
  )
  table <- coef(summary(fit))
  # [/logistic]
} else if (workload == "probit") {
  # [probit]
  fit <- glm(
    y ~ x1 + x2 + x3 + x4 + x5 + x6 + x7 + x8 + x9 + x10,
    data = d,
    family = binomial("probit")
  )
  table <- coef(summary(fit))
  # [/probit]
} else if (workload == "mixed-logistic") {
  # [mixed-logistic]
  fit <- glmer(
    y ~ x1 + x2 + x3 + (1 | g),
    data = d,
    family = binomial,
    control = glmerControl(tolPwrss = 1e-10)
  )
  table <- coef(summary(fit))
  # [/mixed-logistic]
} else {
  # [distributions]
  normal_cdf <- pnorm(x)
  normal_quantile <- qnorm(p)
  gamma_cdf <- pgamma(positive, shape = 2.5, scale = 1.5)
  gamma_quantile <- qgamma(p, shape = 2.5, scale = 1.5)
  # [/distributions]
}
elapsed <- as.numeric(difftime(Sys.time(), start, units = "secs")) * 1000

json <- function(v) paste0("[", paste(sprintf("%.17g", v), collapse = ","), "]")
if (workload == "distributions") {
  every <- function(v) v[seq(1, length(v), by = 1000)]
  cat(sprintf('{"ms":%.6f,"values":%s}\n', elapsed,
    json(c(every(normal_cdf), every(normal_quantile), every(gamma_cdf), every(gamma_quantile)))))
} else {
  cat(sprintf('{"ms":%.6f,"names":[%s],"coefficients":%s,"std_errors":%s}\n', elapsed,
    paste0('"', rownames(table), '"', collapse = ","), json(table[, 1]), json(table[, 2])))
}
