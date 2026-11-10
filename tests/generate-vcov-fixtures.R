# Public sandwich/ivreg APIs and published formulas only. No package source.
library(jsonlite)
library(sandwich)
.libPaths(c('.context/iv-r-library', .libPaths()))
library(ivreg)
set.seed(91093)
n <- 144
base <- data.frame(x = rnorm(n), z1 = rnorm(n), z2 = rnorm(n), z3 = rnorm(n),
                   z4 = rnorm(n), g = rep(c('a', 'b', 'c'), n / 3), day = rep(1:12, each = 12),
                   w = exp(seq(-3, 0, length.out = n)))
shock <- as.numeric(arima.sim(list(ar = .55), n = n)) * (1 + abs(base$x))
base$d <- .8 * base$z1 - .6 * base$z2 + .2 * base$x + .5 * shock + rnorm(n)
base$d2 <- .7 * base$z3 + .5 * base$z4 + .4 * shock + rnorm(n)
base$y <- 2 + .8 * base$x + 1.3 * base$d - .4 * base$d2 + shock
base$o <- .2 * base$x
missing <- base
missing$z1[27] <- NA
missing$x[5] <- NA; missing$w[11] <- NA; missing$day[19] <- NA
iv_fixture <- fromJSON('tests/fixtures/iv-r.json', simplifyVector = FALSE)
heating_case <- Filter(function(c) c$name == 'heating', iv_fixture$cases)[[1]]
heating <- as.data.frame(lapply(heating_case$data, unlist))
heating$day <- rep(seq_len(nrow(heating) / 24), each = 24)
two <- base; two$day <- rep(1:2,each=n/2)
datasets <- list(base = base, missing = missing, heating = heating, two=two)
configs <- list(
 list(name='ols', dataset='base', fun='fit_ols', formula='y ~ x + d + factor(g)', weights=NULL),
 list(name='weighted_ols', dataset='base', fun='fit_ols', formula='y ~ x + d + factor(g)', weights='w'),
 list(name='no_intercept_ols', dataset='base', fun='fit_ols', formula='y ~ 0 + x + d + factor(g)', weights='w'),
 list(name='offset_ols', dataset='base', fun='fit_ols', formula='y ~ x + d + offset(o)', weights='w'),
 list(name='missing_ols', dataset='missing', fun='fit_ols', formula='y ~ x + d', weights='w'),
 list(name='two_clusters', dataset='two', fun='fit_ols', formula='y ~ x + d', weights='w'),
 list(name='missing_iv', dataset='missing', fun='fit_iv', formula='y ~ x | d ~ z1 + z2 + z3', weights='w'),
 list(name='iv', dataset='base', fun='fit_iv', formula='y ~ x + factor(g) | d ~ z1 + z2 + z3', weights=NULL),
 list(name='weighted_iv', dataset='base', fun='fit_iv', formula='y ~ x + factor(g) | d ~ z1 + z2 + z3', weights='w'),
 list(name='no_intercept_iv', dataset='base', fun='fit_iv', formula='y ~ 0 + x | d ~ z1 + z2 + z3', weights='w'),
 list(name='multiple_iv', dataset='missing', fun='fit_iv', formula='y ~ x | d + d2 ~ z1 + z2 + z3 + z4', weights='w'),
 list(name='heating_iv', dataset='heating', fun='fit_iv', formula=heating_case$formula, weights=heating_case$weights),
 list(name='heating_ols', dataset='heating', fun='fit_ols', formula=NA_character_, weights=heating_case$weights))
# Structural OLS equivalent of the saved heating IV formula.
parts <- strsplit(heating_case$formula, '|', fixed=TRUE)[[1]]
endog <- strsplit(parts[2], '~', fixed=TRUE)[[1]][1]
configs[[length(configs)]]$formula <- paste(parts[1], '+', endog)
methods <- list('HC0','HC1','HC2','HC3',list(cluster='day'),list(cluster='day',type='HC0'),
                list(newey_west=0L),list(newey_west=3L),list(newey_west=3L,adjust=TRUE))
make_case <- function(config, option, index) {
 data <- datasets[[config$dataset]]
 is_iv <- config$fun == 'fit_iv'
 f <- if (is_iv) {
   parts <- strsplit(config$formula, '|', fixed=TRUE)[[1]]
   endog <- strsplit(parts[2], '~', fixed=TRUE)[[1]]
   as.formula(paste(parts[1], '+', endog[1], '|', sub('.*~','',parts[1]), '+', endog[2]))
 } else as.formula(config$formula)
 variables <- unique(c(all.vars(f),config$weights,if (is.list(option) && !is.null(option$cluster)) 'day'))
 retained <- which(complete.cases(data[,variables,drop=FALSE]))
 data <- droplevels(data[retained,,drop=FALSE])
 weights <- if (is.null(config$weights)) rep(1,nrow(data)) else data[[config$weights]]
 fit <- if (is_iv) ivreg(f,data=data,weights=weights) else lm(f,data=data,weights=weights)
 cluster <- is.list(option) && !is.null(option$cluster)
 cov <- if (is.character(option)) vcovHC(fit,type=option) else if (cluster) {
   vcovCL(fit,cluster=data$day,type=if(is.null(option$type)) 'HC1' else option$type,cadjust=TRUE)
 } else NeweyWest(fit,lag=option$newey_west,prewhite=FALSE,adjust=isTRUE(option$adjust))
 df <- if (cluster) length(unique(data$day))-1 else df.residual(fit)
 beta <- as.vector(coef(fit)); se <- sqrt(diag(cov)); t <- beta/se
 design <- model.matrix(fit)
 if (is_iv) design <- model.matrix(fit,component='regressors')
 prediction_se <- sqrt(rowSums((design %*% cov)*design))
 list(name=paste0(config$name,'_',index),dataset=config$dataset,fun=config$fun,formula=config$formula,
      weights=config$weights,vcov=option,
      expected=list(coefficients=beta,covariance=unname(cov),std_errors=unname(se),t_statistics=unname(t),
        p_values=unname(2*pt(abs(t),df,lower.tail=FALSE)),conf_int=unname(cbind(beta-qt(.975,df)*se,beta+qt(.975,df)*se)),
        inference_df=df,df_residual=df.residual(fit),retained_rows=retained-1L,fitted=unname(fitted(fit)),residuals=unname(residuals(fit))),
      prediction=list(se_fit=unname(prediction_se),lower=unname(fitted(fit)-qt(.975,df)*prediction_se),upper=unname(fitted(fit)+qt(.975,df)*prediction_se)))
}
cases <- list()
for (config in configs) {
 options <- if (grepl('heating',config$name)) list('HC3',list(cluster='day'),list(newey_west=6L),list(newey_west=24L,adjust=TRUE)) else methods
 for (j in seq_along(options)) cases[[length(cases)+1]] <- make_case(config,options[[j]],j)
}
write_json(list(reference=list(R=R.version.string,sandwich=as.character(packageVersion('sandwich')),ivreg=as.character(packageVersion('ivreg'))),
                datasets=lapply(datasets,as.list),cases=cases), 'tests/fixtures/vcov-r.json',auto_unbox=TRUE,pretty=TRUE,digits=17,na='null',null='null')
cat(length(cases),'covariance references written\n')
