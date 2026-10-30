dnl PIE uses phpize/configure/make; the numerical extension is compiled by Cargo.
PHP_ARG_ENABLE([phit], [whether to enable phit],
  [AS_HELP_STRING([--enable-phit], [Build the Rust phit extension])], [yes])

if test "$PHP_PHIT" != "no"; then
  AC_PATH_PROG([CARGO], [cargo], [])
  if test -z "$CARGO"; then
    AC_MSG_ERROR([cargo is required; install a stable Rust toolchain first])
  fi
  if test "$PHP_THREAD_SAFETY" = "yes"; then
    AC_MSG_ERROR([v0.1 packaging supports non-thread-safe PHP only])
  fi
  STATS_PHP_VERSION=`$PHP_CONFIG --vernum`
  if test "$STATS_PHP_VERSION" -lt 80300 || test "$STATS_PHP_VERSION" -ge 80600; then
    AC_MSG_ERROR([v0.1 packaging targets PHP 8.3, 8.4 and 8.5])
  fi
  STATS_PHP=`$PHP_CONFIG --php-binary`
  AS_CASE([$host_os],
    [linux*], [STATS_DYLIB_SUFFIX=so],
    [darwin*], [STATS_DYLIB_SUFFIX=dylib],
    [AC_MSG_ERROR([v0.1 packaging supports Linux and macOS only])])
  PHP_SUBST([CARGO])
  PHP_SUBST([STATS_PHP])
  PHP_SUBST([STATS_DYLIB_SUFFIX])
  PHP_ADD_MAKEFILE_FRAGMENT([$srcdir/Makefile.frag])
fi
