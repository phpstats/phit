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
  logistic_matrix_mroz = glm(lfp ~ k5 + age, data = mroz, family = binomial),
  probit_matrix_mroz = glm(lfp ~ k5 + age, data = mroz, family = binomial(link = "probit"))
)
rows <- lapply(fits, function(fit) {
  printed <- capture.output(printCoefmat(coef(summary(fit)), digits = 4, signif.stars = FALSE))
  trimws(printed[-1], "right")
})
# Each mixed fit's conditional modes and standard errors, as broom.mixed's tidy(effects = "ran_vals") reads them.
mixed <- c("lmer_sleepstudy_reml", "lmer_sleepstudy_ml", "glmer_cbpp_laplace", "glmer_cbpp_agq10")
ran_vals <- lapply(fits[mixed], function(fit) {
  r <- as.data.frame(ranef(fit, condVar = TRUE))
  list(group = as.character(r$grpvar), level = as.character(r$grp), term = as.character(r$term),
       estimate = r$condval, std_error = r$condsd)
})
writeLines(toJSON(list(R = R.version.string, lme4 = as.character(packageVersion("lme4")), rows = rows,
                       ran_vals = ran_vals), digits = NA,
                  auto_unbox = TRUE, pretty = TRUE), file.path(dir, "summary-r.json"))
