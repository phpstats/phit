# R's printed coefficient rows for the summary fixture cases (tests/fixtures/summary/README.md).
# Run from the repository root: Rscript tests/generate-summary-r.R
suppressPackageStartupMessages({
  library(lme4)
  library(jsonlite)
})
dir <- "tests/fixtures/summary"
mtcars <- read.csv(file.path(dir, "mtcars.csv"))
mroz <- read.csv(file.path(dir, "mroz.csv"))
mroz$lfp <- factor(mroz$lfp)
insurance <- read.csv(file.path(dir, "insurance.csv"))
sleep <- read.csv("docs/data/lmm/sleepstudy.csv")
sleep$Subject <- factor(sleep$Subject, levels = unique(sleep$Subject))
cbpp <- read.csv("docs/data/cbpp.csv")
cbpp$herd <- factor(cbpp$herd, levels = unique(cbpp$herd))
cbpp$period <- factor(cbpp$period)

tight <- glmerControl(tolPwrss = 1e-10)
fits <- list(
  ols_mtcars = lm(mpg ~ wt + factor(cyl), mtcars),
  gaussian_glm_mtcars = glm(mpg ~ wt, data = mtcars),
  logistic_mroz = glm(lfp ~ k5 + age + wc + inc, data = mroz, family = binomial),
  probit_mroz = glm(lfp ~ k5 + age + wc + inc, data = mroz, family = binomial(link = "probit")),
  poisson_insurance = glm(Claims ~ factor(District) + offset(log(Holders)), data = insurance, family = poisson),
  lmer_sleepstudy_reml = lmer(Reaction ~ Days + (Days | Subject), sleep),
  lmer_sleepstudy_ml = lmer(Reaction ~ Days + (1 | Subject), sleep, REML = FALSE),
  glmer_cbpp_laplace = glmer(cbind(incidence, size - incidence) ~ period + (1 | herd), cbpp, binomial, control = tight),
  glmer_cbpp_agq10 = glmer(cbind(incidence, size - incidence) ~ period + (1 | herd), cbpp, binomial, nAGQ = 10,
                           control = tight),
  ols_matrix_mtcars = lm(mpg ~ wt, mtcars),
  logistic_matrix_mroz = glm(lfp ~ k5 + age, data = mroz, family = binomial)
)
rows <- lapply(fits, function(fit) {
  printed <- capture.output(printCoefmat(coef(summary(fit)), digits = 4, signif.stars = FALSE))
  trimws(printed[-1], "right")
})
writeLines(toJSON(list(R = R.version.string, lme4 = as.character(packageVersion("lme4")), rows = rows),
                  auto_unbox = TRUE, pretty = TRUE), file.path(dir, "summary-r.json"))
