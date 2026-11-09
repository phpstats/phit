# References are public APIs and observed output; no reference source is read.
library(jsonlite)
.libPaths(c('.context/iv-r-library', .libPaths()))
library(ivreg)
library(fixest)

# Sanderson & Windmeijer (2016), equations 14-15, after whitening.
# Residual df n-L is the finite-sample convention in published Stata output.
sw_reference <- function(x, z, endogenous, weights) {
  wx <- x * sqrt(weights)
  wz <- z * sqrt(weights)
  controls <- wx[, -endogenous, drop = FALSE]
  d <- wx[, endogenous, drop = FALSE]
  project <- function(a, b) qr.fitted(qr(a), b)
  h <- project(wz, d)
  v <- d - h
  q <- ncol(z) - ncol(controls)
  m <- ncol(d)
  lapply(seq_len(m), function(j) {
    conditional_controls <- cbind(controls, h[, -j, drop = FALSE])
    delta <- lm.fit(conditional_controls, d[, j])$coefficients
    a <- h[, j] - conditional_controls %*% delta
    correction <- if (m > 1) delta[(ncol(controls) + 1):length(delta)] else numeric()
    noise <- v[, j] - v[, -j, drop = FALSE] %*% correction
    statistic <- sum(a^2) / (q - m + 1) / (sum(noise^2) / (nrow(x) - ncol(z)))
    actual_controls <- cbind(controls, d[, -j, drop = FALSE])
    actual_residual <- qr.resid(qr(actual_controls), d[, j])
    fitted_residual <- qr.resid(qr(conditional_controls), h[, j])
    list(sw_f_statistic = statistic, sw_df1 = q - m + 1,
         sw_df2 = nrow(x) - ncol(z),
         sw_p_value = pf(statistic, q - m + 1, nrow(x) - ncol(z), lower.tail = FALSE),
         shea_partial_r_squared = if (m > 1) sum(fitted_residual^2) / sum(actual_residual^2) else NULL)
  })
}

set.seed(7124)
n <- 180
base <- data.frame(x = rnorm(n), z1 = rnorm(n), z2 = rnorm(n), z3 = rnorm(n),
                   z4 = rnorm(n), g = rep(c('a', 'b', 'c'), length.out = n), w = exp(seq(-2, 0, length.out = n)))
u <- rnorm(n)
base$e1 <- 0.8 * base$z1 + 0.5 * base$z2 + u + rnorm(n)
base$e2 <- 0.9 * base$z3 + 0.4 * base$z4 + 0.6 * u + rnorm(n)
base$y <- 2 + 0.7 * base$x + 1.2 * base$e1 - 0.8 * base$e2 + u + rnorm(n)

make_case <- function(name, data, exog, endog, excluded, weighted = FALSE) {
  formula <- paste('y ~', exog, '|', endog, '~', excluded)
  structural <- as.formula(paste('y ~', exog, '+', endog))
  instrument <- as.formula(paste('~', exog, '+', excluded))
  used <- unique(c(all.vars(structural), all.vars(instrument), if (weighted) 'w'))
  retained <- which(complete.cases(data[, used]))
  frame <- droplevels(data[retained, ])
  weights <- if (weighted) frame$w else rep(1, nrow(frame))
  x <- model.matrix(structural, frame)
  z <- model.matrix(instrument, frame)
  exog_columns <- colnames(model.matrix(as.formula(paste('~', exog)), frame))
  endog_indices <- which(!colnames(x) %in% exog_columns)
  reference_formula <- as.formula(paste('y ~', exog, '+', endog, '|', exog, '+', excluded))
  model <- ivreg(reference_formula, data = frame, weights = weights)
  result <- summary(model, diagnostics = TRUE)
  coefs <- result$coefficients
  # fixest uses an explicit small-sample correction to match n-k.
  fixest_formula <- as.formula(formula)
  second_reference <- feols(fixest_formula, frame, weights = weights,
                            vcov = 'iid', ssc = ssc(K.adj = TRUE, K.fixef = 'none'))
  canonical <- function(names) vapply(strsplit(names, ':', fixed = TRUE), function(parts) paste(sort(parts), collapse = ':'), '')
  fixest_order <- match(canonical(colnames(x)), canonical(sub('^fit_', '', names(coef(second_reference)))))
  fixest_stages <- fitstat(second_reference, 'ivf', simplify = FALSE)
  fixest_f <- lapply(fixest_stages, function(test) test$stat)
  names(fixest_f) <- sub('^ivf1::', '', names(fixest_stages))
  first <- sw_reference(x, z, endog_indices, weights)
  for (j in seq_along(first)) {
    response <- x[, endog_indices[j]]
    full <- lm.wfit(z, response, weights)
    restricted <- lm.wfit(x[, colnames(x) %in% exog_columns, drop = FALSE], response, weights)
    rss_full <- sum(weights * full$residuals^2)
    rss_restricted <- sum(weights * restricted$residuals^2)
    q <- ncol(z) - length(exog_columns)
    f <- ((rss_restricted - rss_full) / q) / (rss_full / (nrow(x) - ncol(z)))
    first[[j]] <- c(first[[j]], list(f_statistic = f, df1 = q, df2 = nrow(x) - ncol(z),
      p_value = pf(f, q, nrow(x) - ncol(z), lower.tail = FALSE),
      partial_r_squared = 1 - rss_full / rss_restricted,
      coefficients = as.list(full$coefficients)))
  }
  names(first) <- colnames(x)[endog_indices]
  diagnostic <- result$diagnostics
  wu <- diagnostic['Wu-Hausman', ]
  sargan <- diagnostic['Sargan', ]
  # ivreg 0.6-8 uses a centered denominator even without an intercept and
  # can report a negative Sargan statistic. The specified n*R² auxiliary
  # regression uses an uncentered R² when there is no intercept.
  auxiliary <- lm.wfit(z, residuals(model), weights)
  sargan_stat <- nrow(x) * (1 - sum(weights * auxiliary$residuals^2) / sum(weights * residuals(model)^2))
  printed_rows <- trimws(capture.output(printCoefmat(coefs, digits = 4, signif.stars = FALSE))[-1], 'right')
  prediction_se <- sqrt(rowSums((x %*% vcov(model)) * x))
  critical <- qt(.975, df.residual(model))
  prediction_width <- critical * sqrt(prediction_se^2 + result$sigma^2)
  list(name = name, prediction = list(se_fit = unname(prediction_se),
       confidence_lower = unname(fitted(model) - critical * prediction_se), confidence_upper = unname(fitted(model) + critical * prediction_se),
       prediction_lower = unname(fitted(model) - prediction_width), prediction_upper = unname(fitted(model) + prediction_width)), r_rows = printed_rows, formula = formula, data = as.list(data), weights = if (weighted) 'w' else NULL,
    expected = list(coefficients = unname(coef(model)), covariance = unname(vcov(model)),
      std_errors = unname(coefs[, 2]), t_statistics = unname(coefs[, 3]), p_values = unname(coefs[, 4]),
      conf_int = unname(confint(model)), fitted = unname(fitted(model)), residuals = unname(residuals(model)),
      sigma = result$sigma, rss = sum(weights * residuals(model)^2),
      r_squared = result$r.squared, adj_r_squared = result$adj.r.squared,
      df_residual = df.residual(model), columns = colnames(x), retained_rows = retained - 1L,
      first_stage = first, diagnostics = list(
        wu_hausman = as.list(setNames(unname(wu), c('df1', 'df2', 'statistic', 'p_value'))),
        sargan = if (ncol(z) > ncol(x)) list(statistic = sargan_stat, df = ncol(z) - ncol(x), p_value = pchisq(sargan_stat, ncol(z) - ncol(x), lower.tail = FALSE)) else NULL)),
    ivreg_first_stage = unname(diagnostic[grepl('^Weak instruments', rownames(diagnostic)), , drop = FALSE]),
    fixest = list(coefficients = unname(coef(second_reference)[fixest_order]), std_errors = unname(se(second_reference)[fixest_order]),
                  first_stage_f = fixest_f))
}

cases <- list(
  make_case('no_exog', base, '0', 'e1 + e2', 'z1 + z2 + z3 + z4'),
  make_case('one_exact', base, 'x', 'e1', 'z1'),
  make_case('one_over', base, 'x', 'e1', 'z1 + z2'),
  make_case('two_over', base, 'x', 'e1 + e2', 'z1 + z2 + z3 + z4'),
  make_case('two_exact', base, 'x', 'e1 + e2', 'z1 + z3'),
  make_case('weighted', base, 'x', 'e1 + e2', 'z1 + z2 + z3 + z4', TRUE),
  make_case('no_intercept', base, '0 + x', 'e1 + e2', 'z1 + z2 + z3 + z4'),
  make_case('factors', base, 'x + factor(g)', 'e1', 'z1 + z2'),
  make_case('interactions', base, 'x', 'e1 + e1:x', 'z1 + z2 + z1:x + z2:x'))
missing_data <- base
missing_data$z2[2] <- NA
missing_data$e1[4] <- NA
missing_data$x[7] <- NA
missing_data$w[11] <- NA
cases[[length(cases) + 1]] <- make_case('missing', missing_data, 'x + factor(g)', 'e1', 'z1 + z2', TRUE)
weak <- base
weak$e2 <- weak$e1 + 1e-4 * weak$z3 + rnorm(n)
cases[[length(cases) + 1]] <- make_case('weak_conditional', weak, 'x', 'e1 + e2', 'z1 + z2 + z3 + z4')

# Heating-shaped: 24 hour contrasts, seven floor lags, and two endogenous heat terms.
n_heat <- 720
heating <- data.frame(hour = rep(0:23, length.out = n_heat), outdoor = rnorm(n_heat, 4, 5),
                      schedule1 = rbinom(n_heat, 1, .3), schedule2 = rbinom(n_heat, 1, .5),
                      schedule3 = rbinom(n_heat, 1, .2), w = exp(seq(-3, 0, length.out = n_heat)))
for (lag in 0:6) heating[[paste0('lag', lag)]] <- rnorm(n_heat)
heat_u <- rnorm(n_heat)
heating$hp <- heating$schedule1 + heating$schedule2 + heat_u + rnorm(n_heat)
heating$y <- 1 + .1 * heating$outdoor + .5 * heating$hp - .03 * heating$hp * heating$outdoor + heat_u + rnorm(n_heat)
heat_exog <- paste('outdoor + factor(hour) +', paste(paste0('lag', 0:6), collapse = ' + '))
heat_z <- 'schedule1 + schedule2 + schedule3 + schedule1:outdoor + schedule2:outdoor + schedule3:outdoor'
cases[[length(cases) + 1]] <- make_case('heating', heating, heat_exog, 'hp + hp:outdoor', heat_z, TRUE)

output <- list(reference = list(R = R.version.string, ivreg = as.character(packageVersion('ivreg')),
                fixest = as.character(packageVersion('fixest')),
                sw = 'Sanderson & Windmeijer (2016), equations 14-15; finite-sample n-L divisor'), cases = cases)
write_json(output, 'tests/fixtures/iv-r.json', auto_unbox = TRUE, digits = 17, pretty = TRUE, na = 'null', null = 'null')
