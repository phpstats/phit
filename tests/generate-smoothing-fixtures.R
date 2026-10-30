# Base R only. Reference values for stat_density, stat_bw_nrd0/nrd, stat_approx
# and stat_ecdf. Argument names match the PHP arguments; NA is written as null.
number <- function(x) {
  if (is.na(x)) return('null')
  if (is.infinite(x)) return(if (x > 0) '"Inf"' else '"-Inf"')
  sprintf('%.17g', x)
}
# I() marks a vector that is an array even when it has one element.
json <- function(x) {
  if (is.list(x)) {
    values <- vapply(x, json, "")
    if (!is.null(names(x))) return(paste0('{', paste(paste0('"', names(x), '":', values), collapse=','), '}'))
    return(paste0('[', paste(values, collapse=','), ']'))
  }
  if (is.character(x)) return(paste0('"', x, '"'))
  if (is.logical(x) && !is.na(x)) return(if (x) 'true' else 'false')
  if (length(x) == 1 && !inherits(x, 'AsIs')) return(number(x))
  paste0('[', paste(vapply(unclass(x), number, ""), collapse=','), ']')
}

set.seed(20261030)
normal <- round(rnorm(200, 50, 12), 6)
draws <- rnorm(4000, 0.3, 0.05)
skewed <- round(rexp(300, 0.2), 6)
small <- c(2.5, 3.1, 4.7, 1.2, 9.8, 5.5, 3.3)
counts <- c(rep(3, 40), rep(4, 3), 5, 9)  # IQR is zero, the sd is not
constant <- rep(7.25, 5)                   # bw.nrd0 falls back to abs(x[1])
zeros <- c(0, 0, 0)                        # and then to 1
w <- round(runif(length(small), 0.2, 3), 6); w <- w / sum(w)

# Each case lists the PHP arguments; density() gets the same ones.
density_cases <- list(
  list(name = 'normal', x = normal, args = list()),
  list(name = 'posterior_draws', x = draws, args = list()),
  list(name = 'skewed_nrd_adjust', x = skewed, args = list(bw = 'nrd', adjust = 1.5)),
  list(name = 'counts_iqr_zero', x = counts, args = list()),
  list(name = 'constant', x = constant, args = list()),
  list(name = 'zeros', x = zeros, args = list()),
  list(name = 'fixed_bw_range', x = small, args = list(bw = 0.8, from = 0, to = 12, n = 100)),
  list(name = 'cut_one_large_n', x = small, args = list(cut = 1, n = 1000)),
  list(name = 'weighted', x = small, args = list(weights = I(w))),
  list(name = 'single_point_grid', x = small, args = list(n = 1))
)
for (k in c('epanechnikov', 'rectangular', 'triangular', 'biweight', 'cosine', 'optcosine')) {
  density_cases[[length(density_cases) + 1]] <- list(name = paste0('kernel_', k), x = small,
    args = list(bw = 0.8, kernel = k))
}
density_cases <- lapply(density_cases, function(c) {
  args <- c$args
  if (!is.null(args$weights)) args$weights <- unclass(args$weights)
  d <- do.call(density, c(list(x = c$x), args))
  c$x <- I(c$x)
  c$expected <- list(x = I(d$x), y = I(d$y), bw = d$bw, n = d$n)
  c
})

bandwidth_cases <- lapply(list(normal = normal, draws = draws, skewed = skewed, small = small,
  counts = counts, constant = constant, zeros = zeros, pair = c(1, 2)), function(v) v)
bandwidth_cases <- lapply(names(bandwidth_cases), function(name) {
  v <- bandwidth_cases[[name]]
  list(name = name, x = I(v), nrd0 = bw.nrd0(v), nrd = bw.nrd(v))
})

ax <- c(3, 1, 4, 1.5, 9, 2.6, 5, 3)  # unsorted, with a tie at 3
ay <- c(2, 7, 1, 8, 2, 8, 1, 8)
xout <- c(-1, 1, 1.25, 2, 3, 3.5, 4, 8.999, 9, 12)
approx_cases <- list(
  list(name = 'linear_rule1', args = list()),
  list(name = 'linear_rule2', args = list(rule = 2)),
  list(name = 'constant_left', args = list(method = 'constant', rule = 2)),
  list(name = 'constant_mid', args = list(method = 'constant', f = 0.5)),
  list(name = 'ties_min', args = list(ties = 'min')),
  list(name = 'ties_max', args = list(ties = 'max', rule = 2))
)
approx_cases <- lapply(approx_cases, function(c) {
  args <- c$args
  if (!is.null(args$ties)) args$ties <- match.fun(args$ties)
  c$x <- I(ax); c$y <- I(ay); c$xout <- I(xout)
  c$expected <- I(suppressWarnings(do.call(approx, c(list(x = ax, y = ay, xout = xout), args)))$y)
  c
})
# A density read back at points, as the port does.
d <- density(draws)
at <- quantile(draws, c(0.025, 0.1, 0.5, 0.9, 0.975), names = FALSE)
approx_cases[[length(approx_cases) + 1]] <- list(name = 'density_heights', args = list(),
  x = I(d$x), y = I(d$y), xout = I(at), expected = I(approx(d$x, d$y, at)$y))

ecdf_cases <- list(
  list(name = 'small', x = I(c(3, 1, 2, 2, 5)), q = I(c(-1, 1, 1.5, 2, 4.99, 5, 6))),
  list(name = 'draws', x = I(draws), q = I(c(0.2, 0.25, 0.3, 0.35, 0.4)))
)
ecdf_cases <- lapply(ecdf_cases, function(c) {
  c$expected <- I(ecdf(unclass(c$x))(unclass(c$q)))
  c
})

out <- list(r_version = R.version.string, seed = 20261030,
  density = density_cases, bandwidth = bandwidth_cases, approx = approx_cases, ecdf = ecdf_cases)
writeLines(json(out), 'tests/fixtures/smoothing-r.json')
