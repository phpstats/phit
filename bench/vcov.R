library(jsonlite)
library(sandwich)
.libPaths(c('.context/iv-r-library',.libPaths()))
library(ivreg)
fixture <- fromJSON('tests/fixtures/vcov-r.json',simplifyVector=FALSE)
name <- commandArgs(trailingOnly=TRUE)[1]
case <- Filter(function(c)c$name==name,fixture$cases)[[1]]
data <- as.data.frame(lapply(fixture$datasets[[case$dataset]],unlist))
is_iv <- case$fun=='fit_iv'
f <- if(is_iv){
 parts <- strsplit(case$formula,'|',fixed=TRUE)[[1]]
 endog <- strsplit(parts[2],'~',fixed=TRUE)[[1]]
 as.formula(paste(parts[1],'+',endog[1],'|',sub('.*~','',parts[1]),'+',endog[2]))
} else as.formula(case$formula)
weights <- if(is.null(case$weights))rep(1,nrow(data)) else data[[case$weights]]
option <- case$vcov
start <- proc.time()[['elapsed']]
fit <- if(is_iv)ivreg(f,data=data,weights=weights)else lm(f,data=data,weights=weights)
if(is_iv) diagnostics <- summary(fit,diagnostics=TRUE)
cov <- if(is.character(option))vcovHC(fit,type=option)else if(!is.null(option$cluster)){
 vcovCL(fit,cluster=data$day,type='HC1',cadjust=TRUE)
} else NeweyWest(fit,lag=option$newey_west,prewhite=FALSE,adjust=isTRUE(option$adjust))
elapsed <- (proc.time()[['elapsed']]-start)*1000
cat(toJSON(list(ms=elapsed,coefficients=c(as.vector(coef(fit)),as.vector(t(cov))),n=nrow(data),k=length(coef(fit))),auto_unbox=TRUE,digits=17))
