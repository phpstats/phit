# Use the workspace target so direct Cargo and PIE builds can share compilation.
STATS_CARGO_TARGET_DIR = $(top_srcdir)/../target
STATS_ARTIFACT = $(STATS_CARGO_TARGET_DIR)/release/libphit.$(STATS_DYLIB_SUFFIX)
STATS_MODULE = $(top_builddir)/modules/phit.so

.PHONY: stats-rust-build stats-artifact-check install-stats-rust
all: stats-rust-build

# make install may inherit an `all` prerequisite from phpize. Never compile as
# root: require an artifact already built by the unprivileged make step.
ifneq ($(filter install install-modules install-stats-rust,$(MAKECMDGOALS)),)
stats-rust-build: stats-artifact-check
else
stats-rust-build:
	PHP="$(STATS_PHP)" PHP_CONFIG="$(STATS_PHP_CONFIG)" CARGO_TARGET_DIR="$(STATS_CARGO_TARGET_DIR)" "$(CARGO)" build --manifest-path "$(top_srcdir)/../Cargo.toml" -p phit --lib --release --locked
	@mkdir -p "$(top_builddir)/modules"
	cp "$(STATS_ARTIFACT)" "$(STATS_MODULE)"
endif

stats-artifact-check:
	@test -f "$(STATS_MODULE)" || { echo 'Build the extension with make before running make install.' >&2; exit 1; }

install: install-stats-rust
install-stats-rust: stats-artifact-check
	@mkdir -p "$(INSTALL_ROOT)$(EXTENSION_DIR)"
	$(INSTALL) -m 755 "$(STATS_MODULE)" "$(INSTALL_ROOT)$(EXTENSION_DIR)/phit.so"
