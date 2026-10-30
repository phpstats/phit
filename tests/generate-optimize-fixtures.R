# Base R only. Reference results for Phit\optimize and Phit\uniroot. Each
# function has a PHP twin in tests/optimize.php, written with the same
# operations so both sides evaluate the same doubles. Counting evaluations
# checks that the search visits as many points as R's.
number <- function(x) {
  if (is.na(x)) return('null')
  if (is.infinite(x)) return(if (x > 0) '"Inf"' else '"-Inf"')
  sprintf('%.17g', x)
}
json <- function(x) {
  if (is.list(x)) {
    values <- vapply(x, json, "")
    if (!is.null(names(x))) return(paste0('{', paste(paste0('"', names(x), '":', values), collapse=','), '}'))
    return(paste0('[', paste(values, collapse=','), ']'))
  }
  if (is.character(x)) return(paste0('"', x, '"'))
  if (is.logical(x)) return(if (x) 'true' else 'false')
  number(x)
}

functions <- list(
  parabola = function(x) (x - 1/3) * (x - 1/3),
  sine = function(x) sin(x),
  kink = function(x) abs(x - 0.7),
  flat = function(x) 1,
  linear = function(x) x,
  decay = function(x) exp(-x),
  double_well = function(x) (x * x - 1) * (x * x - 1) + 0.3 * x,
  wall = function(x) if (x < 0.45) Inf else (x - 0.5) * (x - 0.5),
  far = function(x) (x - 1e6) * (x - 1e6),
  hump = function(x) x * exp(-x),
  sqrt2 = function(x) x * x - 2,
  fixed_point = function(x) cos(x) - x,
  exp5 = function(x) exp(x) - 5,
  at_one = function(x) x - 1,
  at_three = function(x) x - 3,
  step = function(x) if (x < 0.3) -1 else 1,
  cubic = function(x) x * x * x - 2 * x - 5,
  log = function(x) log(x),
  cliff = function(x) if (x < 0.5) -Inf else x - 0.6
)
counted <- function(name) {
  calls <- 0
  list(f = function(x) { calls <<- calls + 1; functions[[name]](x) }, calls = function() calls)
}

optimize_cases <- list(
  list(f = 'parabola', lower = 0, upper = 1),
  list(f = 'parabola', lower = 0, upper = 1, tol = 1e-10),
  list(f = 'sine', lower = 0, upper = 6),
  list(f = 'kink', lower = 0, upper = 1),
  list(f = 'flat', lower = 0, upper = 1),
  list(f = 'linear', lower = 0, upper = 1),
  list(f = 'decay', lower = 0, upper = 5),
  list(f = 'double_well', lower = -2, upper = 2),
  list(f = 'wall', lower = 0, upper = 1),
  list(f = 'far', lower = 0, upper = 2e6),
  list(f = 'hump', lower = 0, upper = 5, maximum = TRUE),
  list(f = 'sine', lower = 0, upper = 6, maximum = TRUE, tol = 1e-8)
)
optimize_cases <- lapply(optimize_cases, function(c) {
  k <- counted(c$f)
  args <- list(f = k$f, lower = c$lower, upper = c$upper, maximum = isTRUE(c$maximum))
  if (!is.null(c$tol)) args$tol <- c$tol
  r <- suppressWarnings(do.call(optimize, args))
  c$expected <- list(x = if (isTRUE(c$maximum)) r$maximum else r$minimum, objective = r$objective,
    evaluations = k$calls())
  c
})

uniroot_cases <- list(
  list(f = 'sqrt2', lower = 0, upper = 2),
  list(f = 'sqrt2', lower = 0, upper = 2, tol = 1e-12),
  list(f = 'fixed_point', lower = 0, upper = 1),
  list(f = 'exp5', lower = 0, upper = 3, tol = 1e-12),
  list(f = 'at_one', lower = 1, upper = 3),
  list(f = 'at_three', lower = 1, upper = 3),
  list(f = 'step', lower = 0, upper = 1),
  list(f = 'cubic', lower = 2, upper = 3),
  list(f = 'cubic', lower = 2, upper = 3, maxiter = 3),
  list(f = 'log', lower = 0, upper = 2),
  list(f = 'cliff', lower = 0, upper = 1)
)
uniroot_cases <- lapply(uniroot_cases, function(c) {
  k <- counted(c$f)
  args <- list(f = k$f, lower = c$lower, upper = c$upper)
  if (!is.null(c$tol)) args$tol <- c$tol
  if (!is.null(c$maxiter)) args$maxiter <- c$maxiter
  converged <- TRUE
  r <- withCallingHandlers(do.call(uniroot, args), warning = function(w) {
    if (grepl('NOT_ converged', conditionMessage(w))) converged <<- FALSE
    invokeRestart('muffleWarning')
  })
  c$expected <- list(root = r$root, f_root = r$f.root, iter = r$iter, estim_prec = r$estim.prec,
    converged = converged, evaluations = k$calls())
  c
})

writeLines(json(list(r_version = R.version.string, optimize = optimize_cases, uniroot = uniroot_cases)),
  'tests/fixtures/optimize-r.json')
