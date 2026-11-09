# Public reference APIs only. No glmnet, MASS, lmridge, or R source is inspected.
library(jsonlite)
library(glmnet)
.libPaths(c('.context/ridge-r-library', .libPaths()))
library(lmridge)
set.seed(9307)
n <- 96
base <- data.frame(a = rnorm(n, 2, 3), b = rnorm(n, -1, 2), c = rnorm(n),
                   w = exp(seq(-3, 0, length.out = n)), g = rep(c('a', 'b', 'c'), n / 3))
base$y <- 7 + 1.2 * base$a - .8 * base$b + .3 * base$c + rnorm(n, 0, 2)

make_fit <- function(name, data, formula = 'y ~ a + b + c', lambda = .7,
                     standardize = TRUE, weighted = FALSE, factors = list(), bounds = list()) {
  variables <- unique(c(all.vars(as.formula(formula)), if (weighted) 'w'))
  retained <- which(complete.cases(data[, variables]))
  d <- droplevels(data[retained, ])
  design <- model.matrix(as.formula(formula), d)
  intercept <- '(Intercept)' %in% colnames(design)
  x <- design[, colnames(design) != '(Intercept)', drop = FALSE]
  y <- d$y
  weights <- if (weighted) d$w else rep(1, nrow(d))
  pf <- rep(1, ncol(x)); names(pf) <- colnames(x)
  lower <- rep(-Inf, ncol(x)); upper <- rep(Inf, ncol(x))
  for (pattern in names(factors)) {
    matched <- grepl(glob2rx(pattern), colnames(x))
    pf[matched] <- factors[[pattern]]
  }
  for (pattern in names(bounds)) {
    matched <- grepl(glob2rx(pattern), colnames(x))
    if (!is.null(bounds[[pattern]][[1]])) lower[matched] <- bounds[[pattern]][[1]]
    if (!is.null(bounds[[pattern]][[2]])) upper[matched] <- bounds[[pattern]][[2]]
  }
  reference <- glmnet(x, y, alpha = 0, lambda = lambda, weights = weights,
    intercept = intercept, standardize = standardize, penalty.factor = pf,
    lower.limits = lower, upper.limits = upper, thresh = 1e-18, maxit = 1000000)
  coefficients <- as.vector(coef(reference))
  if (!intercept) coefficients <- coefficients[-1]
  fitted <- as.vector(predict(reference, newx = x))
  # Independent conditional-on-lambda covariance and local active-face df.
  q <- weights / sum(weights)
  y_mean <- if (intercept) sum(q * y) else 0
  sy <- sqrt(sum(q * (y - y_mean)^2))
  means <- if (intercept) colSums(x * q) else rep(0, ncol(x))
  centered <- sweep(x, 2, colSums(x * q))
  scales <- if (standardize) sqrt(colSums(centered^2 * q)) else rep(1, ncol(x))
  z <- sweep(sweep(x, 2, means), 2, scales, '/')
  if (intercept) z <- cbind('(Intercept)' = 1, z)
  penalty <- pf * ncol(x) / sum(pf)
  if (intercept) penalty <- c(0, penalty)
  active <- (as.vector(coef(reference))[-1] == lower | as.vector(coef(reference))[-1] == upper)
  if (intercept) active <- c(FALSE, active)
  free <- which(!active)
  gram <- crossprod(z[, free, drop = FALSE], q * z[, free, drop = FALSE])
  inverse <- solve(gram + diag(lambda / sy * penalty[free], length(free)))
  df <- sum(diag(inverse %*% gram))
  rss <- sum(q * length(y) * (y - fitted)^2)
  sigma <- sqrt(rss / (length(y) - df))
  transform <- diag(1, ncol(z))
  slopes <- if (intercept) 2:ncol(z) else seq_len(ncol(z))
  transform[cbind(slopes, slopes)] <- 1 / scales
  if (intercept) transform[1, slopes] <- -means / scales
  transform <- transform[, free, drop = FALSE]
  covariance <- transform %*% inverse %*% gram %*% inverse %*% t(transform) * sigma^2 / length(y)
  covariance[active, ] <- NA; covariance[, active] <- NA
  se <- sqrt(diag(covariance))
  critical <- qt(.975, length(y) - df)
  ci <- cbind(coefficients - critical * se, coefficients + critical * se)
  mass_lambda <- if (intercept && standardize && !weighted && length(bounds) == 0 && all(penalty[-1] == 1)) length(y) * lambda / sy else NULL
  mass <- if (!is.null(mass_lambda)) as.vector(coef(MASS::lm.ridge(as.formula(formula), d, lambda = mass_lambda))) else NULL
  table <- cbind(Estimate = coefficients, 'Std. Error' = se, 't value' = coefficients / se,
                 'Pr(>|t|)' = 2 * pt(abs(coefficients / se), length(y) - df, lower.tail = FALSE))
  rownames(table) <- colnames(design)
  r_rows <- trimws(capture.output(printCoefmat(table, digits = 4, signif.stars = FALSE))[-1], 'right')
  list(name = name, r_rows = r_rows, formula = formula, data = as.list(data), lambda = lambda,
       standardize = standardize, weights = if (weighted) 'w' else NULL,
       penaltyFactor = factors, bounds = bounds,
       expected = list(coefficients = coefficients, fitted = fitted, residuals = y - fitted,
         columns = colnames(design), retained_rows = retained - 1L, rss = rss, sigma = sigma,
         df = df, df_residual = length(y) - df, r_squared = 1 - rss / (length(y) * sy^2),
         covariance = unname(covariance), std_errors = unname(se), conf_int = lapply(seq_len(ncol(z)), function(j) if (active[j]) NULL else unname(ci[j, ])),
         penalty_factor = unname(penalty), at_bound = unname(active),
         lambda_effective = lambda / sy, response_scale = sy, lambda_lm_ridge = mass_lambda),
       MASS = mass)
}
fits <- list(
  make_fit('standardized', base), make_fit('raw', base, standardize = FALSE),
  make_fit('weighted', base, weighted = TRUE),
  make_fit('no_intercept', base, 'y ~ 0 + a + b + c'),
  make_fit('no_intercept_raw', base, 'y ~ 0 + a + b + c', standardize = FALSE),
  make_fit('unpenalized', base, factors = list(a = 0, b = 2)),
  make_fit('bounded', base, bounds = list(b = list(0, NULL), a = list(NULL, .5))),
  make_fit('bounded_zero_lambda', base, lambda = 0, bounds = list(b = list(0, NULL))),
  make_fit('factors_interactions', base, 'y ~ a + factor(g) + a:b', factors = list('factor(g)*' = 0)),
  make_fit('weighted_bounded', base, weighted = TRUE, bounds = list(b = list(0, NULL))))
missing_data <- base
missing_data$a[2] <- NA; missing_data$w[5] <- NA
fits[[length(fits) + 1]] <- make_fit('missing', missing_data, weighted = TRUE)

# Independent lmridge variance convention is recorded separately from phit's
# specified RSS/(n-df) estimate. vcov is descaled back to predictor units.
lmr_k <- fits[[1]]$lambda / fits[[1]]$expected$response_scale
lmr <- lmridge(y ~ a + b + c, base, K = lmr_k, scaling = 'sc')
lmr_cov <- vcov(lmr)[[1]]
lmr_scale <- as.numeric(lmr$xscale)
lmr_slopes <- as.numeric(lmr$coef[, 1]) / lmr_scale
lmr_coefficients <- c(mean(base$y) - sum(colMeans(base[c('a','b','c')]) * lmr_slopes), lmr_slopes)
lmr_hat <- hatr(lmr)[[1]]
lmr_residual <- base$y - as.vector(cbind(1, as.matrix(base[c('a','b','c')])) %*% lmr_coefficients)
lmr_sigma2 <- sum(lmr_residual^2) / (n - 2 * sum(diag(lmr_hat)) + sum(lmr_hat^2))
lmr_record <- list(K = lmr_k, coefficients = lmr_coefficients,
  covariance = unname(lmr_cov / outer(lmr_scale, lmr_scale)), sigma2 = lmr_sigma2,
  scales = lmr_scale, rounded_public_coef = as.vector(coef(lmr)))

make_cv <- function(name, fit, foldid, lambda = c(3, 1, .3, .1, .03, .01)) {
  original_foldid <- foldid
  d <- as.data.frame(fit$data)
  variables <- unique(c(all.vars(as.formula(fit$formula)), if (!is.null(fit$weights)) fit$weights))
  retained <- which(complete.cases(d[, variables]))
  d <- droplevels(d[retained, ])
  foldid <- foldid[retained]
  design <- model.matrix(as.formula(fit$formula), d)
  x <- design[, colnames(design) != '(Intercept)', drop = FALSE]
  weights <- if (is.null(fit$weights)) rep(1, nrow(d)) else d[[fit$weights]]
  pf <- rep(1, ncol(x))
  for (pattern in names(fit$penaltyFactor)) pf[grepl(glob2rx(pattern), colnames(x))] <- fit$penaltyFactor[[pattern]]
  lower <- rep(-Inf, ncol(x)); upper <- rep(Inf, ncol(x))
  for (pattern in names(fit$bounds)) {
    matched <- grepl(glob2rx(pattern), colnames(x))
    if (!is.null(fit$bounds[[pattern]][[1]])) lower[matched] <- fit$bounds[[pattern]][[1]]
    if (!is.null(fit$bounds[[pattern]][[2]])) upper[matched] <- fit$bounds[[pattern]][[2]]
  }
  reference <- cv.glmnet(x, d$y, alpha = 0, lambda = lambda, foldid = foldid,
    weights = weights, standardize = fit$standardize, intercept = '(Intercept)' %in% colnames(design),
    penalty.factor = pf, lower.limits = lower, upper.limits = upper, grouped = TRUE,
    keep = TRUE, thresh = 1e-18, maxit = 1000000)
  fold_weights <- vapply(sort(unique(foldid)), function(k) sum(weights[foldid == k]), 0.)
  squared_error <- (reference$fit.preval - d$y)^2
  fold_mse <- sapply(sort(unique(foldid)), function(k) colSums(squared_error[foldid == k, , drop = FALSE] * weights[foldid == k]) / sum(weights[foldid == k]))
  fit$lambda <- NULL; fit$expected <- NULL; fit$MASS <- NULL
  c(fit, list(cv_name = name, lambda = lambda, foldid = original_foldid,
    expected_cv = list(lambda = reference$lambda, cvm = reference$cvm, cvsd = reference$cvsd,
      cvup = reference$cvup, cvlo = reference$cvlo, lambda_min = reference$lambda.min,
      lambda_1se = reference$lambda.1se, index_min = which.min(reference$cvm) - 1L,
      index_1se = which(reference$lambda == reference$lambda.1se) - 1L),
    observed_fold_weights = fold_weights, observed_fold_mse = unname(fold_mse)))
}
cv <- list(make_cv('small_folds_ungrouped', make_fit('small', base[1:8, ]), rep(1:4, each = 2)),
           make_cv('unequal_folds_and_weights', fits[[3]], rep(1:4, c(12, 18, 24, 42))),
           make_cv('bounded_weighted', fits[[10]], rep(1:4, 24)),
           make_cv('no_intercept', fits[[4]], rep(1:4, 24)),
           make_cv('missing_original_rows', fits[[11]], rep(1:4, 24)))

# Hourly rows, day folds, age-decaying weights, factors, and collinear floor lags.
n_heat <- 288
heating <- data.frame(hour = rep(0:23, 12), outdoor = rnorm(n_heat, 4, 5), hp = rgamma(n_heat, 2),
                      w = exp(seq(-3, 0, length.out = n_heat)))
latent <- rnorm(n_heat)
for (lag in 0:6) heating[[paste0('floor_call_lag', lag)]] <- latent + rnorm(n_heat, 0, .2)
heating$y <- .1 * heating$outdoor + .5 * heating$hp + .2 * latent + rnorm(n_heat)
formula <- paste('y ~ outdoor + hp + factor(hour) +', paste(paste0('floor_call_lag', 0:6), collapse = ' + '))
heat_fit <- make_fit('heating', heating, formula, weighted = TRUE,
                     factors = list('factor(hour)*' = 0), bounds = list('floor_call_lag*' = list(0, NULL)))
fits[[length(fits) + 1]] <- heat_fit
cv[[length(cv) + 1]] <- make_cv('day_folds_with_age_weights', heat_fit, rep(rep(1:4, 3), each = 24), c(3, 1, .3, .1, .03))
write_json(list(reference = list(R = R.version.string, glmnet = as.character(packageVersion('glmnet')),
  MASS = as.character(packageVersion('MASS')), lmridge = as.character(packageVersion('lmridge'))),
  fits = fits, cv = cv, lmridge = lmr_record), 'tests/fixtures/ridge-r.json',
  auto_unbox = TRUE, digits = 17, pretty = TRUE, na = 'null', null = 'null')
