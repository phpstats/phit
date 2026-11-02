# Input/output tables of R's observable behavior for the clean-room specs in this directory.
# Run from the repository root: Rscript specs/cleanroom/generate-r-tables.R
# Only R's results are recorded; nothing here reads or reflects R's source code.
suppressPackageStartupMessages(library(jsonlite))
out <- "specs/cleanroom/data"
set.seed(20261102)
save <- function(x, name) writeLines(toJSON(x, digits = NA, auto_unbox = TRUE, null = "null", na = "string",
                                            pretty = FALSE), file.path(out, name))
num <- function(x) ifelse(is.na(x) & !is.nan(x), NA, x)

# format(x, digits = d): one common layout for the vector.
magnitudes <- c(1e-320, 1e-300, 1e-120, 1e-20, 1e-8, 1e-5, 1e-4, 1e-3, 0.01, 0.1, 1, 10, 100, 1e3, 1e4, 1e5, 1e6,
                1e7, 1e15, 1e22, 1e23, 1e100, 1e300)
format_cases <- list()
add <- function(x, d) format_cases[[length(format_cases) + 1]] <<- list(x = as.list(sprintf("%.17g", x)), digits = d,
                                                                        out = format(x, digits = d))
for (d in c(1, 2, 3, 4, 5, 7, 10, 15)) {
  for (k in 1:60) {
    n <- sample(1:5, 1)
    x <- sample(c(-1, 1), n, replace = TRUE) * sample(magnitudes, n, replace = TRUE) * runif(n, 0.5, 9.99)
    if (runif(1) < 0.2) x[1] <- 0
    if (runif(1) < 0.2) x[n] <- round(x[n], sample(0:3, 1))
    add(x, d)
  }
}
specials <- list(c(99996), c(99999.6), c(9999.6), c(999.96), c(99999.6, 1.5), c(9999.6, 1), c(0.1 + 0.2), c(123456789),
                 c(0.000123456), c(1e-10, 0.5), c(-0.00001234, 12), c(100, 1e-8), c(0, -0), c(1, NaN, Inf), c(-Inf, 2.5),
                 c(1378.2, 960.5), c(2.220446049250313e-16), c(5e-324), c(1.7976931348623157e308), c(0.5, 0.25, 0.125),
                 c(1e5, 123456), c(1e15, 1), c(3.14159, 2.71828, 1.41421), c(1e-300, 1e300), c(9.995, 0.0001))
for (x in specials) for (d in c(1, 3, 4, 5, 7)) add(x, d)
save(format_cases, "format.json")

# formatC(x, digits = d): C's %g.
g_cases <- list()
for (d in c(1, 2, 3, 4, 6)) for (x in c(0, 0.83743, 0.82, 48.08, 1.5e-7, 123456, 0.0001234, 100, -2.5e-5, 9.99996, 99999.5,
                                         1e-300, 1e300, 0.1 + 0.2, NaN, Inf, -Inf, 5e-324))
  g_cases[[length(g_cases) + 1]] <- list(x = sprintf("%.17g", x), digits = d, out = formatC(x, digits = d))
save(g_cases, "format_g.json")

# format.pval(p, digits = d), eps = machine epsilon.
p_cases <- list()
pv <- c(1e-30, 1e-17, 2e-16, 3e-16, 1e-10, 1e-6, 4.9e-5, 9.99e-5, 1e-4, 0.000213, 0.001, 0.004718, 0.04, 0.05, 0.3,
        0.5, 0.999, 1, NaN)
for (d in c(1, 2, 3, 4, 5)) {
  for (k in 1:40) {
    n <- sample(1:5, 1)
    p <- sample(pv, n, replace = TRUE) * ifelse(runif(n) < 0.5, 1, runif(n, 0.5, 1))
    p_cases[[length(p_cases) + 1]] <- list(p = as.list(sprintf("%.17g", p)), digits = d, out = format.pval(p, digits = d))
  }
}
save(p_cases, "format_pval.json")

# round(x, d).
r_cases <- list()
for (k in 1:300) {
  d <- sample(0:8, 1)
  x <- sample(c(-1, 1), 1) * runif(1, 0, 10^sample(-3:6, 1))
  if (k %% 5 == 0) x <- (floor(x * 10^d) + 0.5) / 10^d
  r_cases[[length(r_cases) + 1]] <- list(x = sprintf("%.17g", x), digits = d, out = sprintf("%.17g", round(x, d)))
}
save(r_cases, "round.json")

# printCoefmat(digits = 4, signif.stars = FALSE): the printed rows of random coefficient tables.
coef_cases <- list()
for (k in 1:120) {
  p <- sample(1:6, 1)
  scale <- 10^sample(-4:4, 1)
  est <- rnorm(p) * scale * 10^sample(-1:2, p, replace = TRUE)
  se <- abs(rnorm(p, 1, 0.3)) * scale * 10^sample(-2:1, p, replace = TRUE)
  if (k %% 10 == 0) est[1] <- 0
  stat <- est / se
  pval <- 2 * pnorm(-abs(stat))
  m <- cbind(Estimate = est, "Std. Error" = se, "z value" = stat, "Pr(>|z|)" = pval)
  rownames(m) <- paste0("b", seq_len(p))
  rows <- capture.output(printCoefmat(m, digits = 4, signif.stars = FALSE))
  coef_cases[[length(coef_cases) + 1]] <- list(estimate = as.list(sprintf("%.17g", est)),
                                               std_error = as.list(sprintf("%.17g", se)),
                                               statistic = as.list(sprintf("%.17g", stat)),
                                               p_value = as.list(sprintf("%.17g", pval)), printed = rows)
}
save(coef_cases, "coefmat.json")

# approx(): sorted unique x, all methods and rules.
a_cases <- list()
for (k in 1:80) {
  n <- sample(2:8, 1)
  x <- sort(unique(round(runif(n, -5, 5), sample(0:3, 1))))
  if (length(x) < 2) next
  y <- rnorm(length(x)) * 10^sample(-2:3, 1)
  if (k %% 7 == 0) y[1] <- Inf
  xout <- c(x, runif(6, min(x) - 1, max(x) + 1), NaN)
  method <- if (k %% 2 == 0) "linear" else "constant"
  f <- if (method == "constant") sample(c(0, 0.25, 0.5, 1), 1) else 0
  rule <- sample(1:2, 1)
  r <- approx(x, y, xout = xout, method = method, f = f, rule = rule, ties = "ordered")$y
  a_cases[[length(a_cases) + 1]] <- list(x = as.list(sprintf("%.17g", x)), y = as.list(sprintf("%.17g", y)),
                                         xout = as.list(sprintf("%.17g", xout)), method = method, f = f, rule = rule,
                                         out = as.list(sprintf("%.17g", r)))
}
save(a_cases, "approx.json")

# quantile(type = 7).
q_cases <- list()
for (k in 1:150) {
  n <- sample(1:12, 1)
  x <- round(rnorm(n) * 10^sample(-2:4, 1), sample(0:6, 1))
  if (k %% 6 == 0) x <- rep(x[1], n)
  p <- c(0, 1, runif(4), 0.5, 0.25, 0.75)
  q_cases[[length(q_cases) + 1]] <- list(x = as.list(sprintf("%.17g", x)), p = as.list(sprintf("%.17g", p)),
                                         out = as.list(sprintf("%.17g", unname(quantile(x, p, type = 7)))))
}
save(q_cases, "quantile7.json")

# dnorm(z) and dnorm(x, 0, sigma).
z <- c(0, 1e-300, 1e-8, 0.5, 1, 2, 4.999, 5, 5.001, 7, 10, 20, 30, 37, 38, 38.5, 40, 1e10, -3, -38.2, Inf, -Inf, NaN,
       runif(80, -40, 40))
sigma <- c(rep(1, length(z)))
d_cases <- list(z = as.list(sprintf("%.17g", z)), out = as.list(sprintf("%.17g", dnorm(z))))
xs <- runif(60, -50, 50); ss <- 10^runif(60, -3, 3)
d_cases$scaled <- list(x = as.list(sprintf("%.17g", xs)), sigma = as.list(sprintf("%.17g", ss)),
                       out = as.list(sprintf("%.17g", dnorm(xs, 0, ss))))
save(d_cases, "dnorm.json")

writeLines(R.version.string, file.path(out, "R_VERSION"))
