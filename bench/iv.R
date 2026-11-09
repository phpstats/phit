args <- commandArgs(trailingOnly = TRUE)
.libPaths(c('.context/iv-r-library', .libPaths()))
library(jsonlite)
library(ivreg)
fixture <- fromJSON('tests/fixtures/iv-r.json', simplifyVector = FALSE)
name <- args[1]
copies <- if (name == 'heating_2160') 3 else 1
source_name <- if (copies == 3) 'heating' else name
case <- Filter(function(case) case$name == source_name, fixture$cases)[[1]]
data <- as.data.frame(lapply(case$data, function(column) rep(unlist(column), copies)))
parts <- strsplit(case$formula, '|', fixed = TRUE)[[1]]
first <- strsplit(parts[2], '~', fixed = TRUE)[[1]]
f <- as.formula(paste(parts[1], '+', first[1], '|', sub('.*~', '', parts[1]), '+', first[2]))
weights <- if (is.null(case$weights)) rep(1, nrow(data)) else data[[case$weights]]
start <- proc.time()[['elapsed']]
model <- ivreg(f, data = data, weights = weights)
result <- summary(model, diagnostics = TRUE)
elapsed <- (proc.time()[['elapsed']] - start) * 1000
cat(toJSON(list(ms = elapsed, coefficients = unname(coef(model)), n = nobs(model), k = length(coef(model))), auto_unbox = TRUE, digits = 17))
