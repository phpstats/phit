library(jsonlite)
library(glmnet)
name <- commandArgs(trailingOnly = TRUE)[1]
fixture <- fromJSON('tests/fixtures/ridge-r.json', simplifyVector = FALSE)
is_cv <- name == 'cv_heating'
copies <- if (name == 'heating_1152') 4 else 1
source_name <- if (is_cv) 'day_folds_with_age_weights' else if (name == 'standardized') name else 'heating'
case <- Filter(function(c) c[[if (is_cv) 'cv_name' else 'name']] == source_name,
               fixture[[if (is_cv) 'cv' else 'fits']])[[1]]
data <- as.data.frame(lapply(case$data, function(column) rep(unlist(column), copies)))
start <- proc.time()[['elapsed']]
design <- model.matrix(as.formula(case$formula), data)
intercept <- '(Intercept)' %in% colnames(design)
x <- design[, colnames(design) != '(Intercept)', drop = FALSE]
pf <- rep(1, ncol(x)); lower <- rep(-Inf, ncol(x)); upper <- rep(Inf, ncol(x))
for (pattern in names(case$penaltyFactor)) pf[grepl(glob2rx(pattern), colnames(x))] <- case$penaltyFactor[[pattern]]
for (pattern in names(case$bounds)) {
 matched <- grepl(glob2rx(pattern), colnames(x))
 if (!is.null(case$bounds[[pattern]][[1]])) lower[matched] <- case$bounds[[pattern]][[1]]
 if (!is.null(case$bounds[[pattern]][[2]])) upper[matched] <- case$bounds[[pattern]][[2]]
}
weights <- if (is.null(case$weights)) rep(1, nrow(data)) else data[[case$weights]]
options <- list(x=x,y=data$y,alpha=0,lambda=unlist(case$lambda),weights=weights,
 intercept=intercept,standardize=case$standardize,penalty.factor=pf,
 lower.limits=lower,upper.limits=upper,control=list(thresh=1e-18,maxit=1000000))
if (is_cv) {
 options$foldid <- unlist(case$foldid)
 result <- do.call(cv.glmnet,options)
 coefficients <- c(as.vector(coef(result,s='lambda.min')),result$cvm,result$cvsd,result$lambda.min,result$lambda.1se)
} else coefficients <- as.vector(coef(do.call(glmnet,options)))
if (!intercept) coefficients <- coefficients[-1]
elapsed <- (proc.time()[['elapsed']]-start)*1000
cat(toJSON(list(ms=elapsed,coefficients=coefficients,n=nrow(data),k=ncol(design)),auto_unbox=TRUE,digits=17))
