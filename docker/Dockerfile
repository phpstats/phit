# syntax=docker/dockerfile:1
# Both build and runtime use the same PHP image/ABI. Update these digest pins
# deliberately and rerun the deployment gate; Cargo.lock pins Rust dependencies.
ARG PHP_IMAGE=php:8.3-fpm-bookworm@sha256:f768bd9142444f6126a690697b028c23f30878c3b628a7c355dfce75e29cc045
ARG RUST_IMAGE=rust:1-bookworm@sha256:114c7a4425406451c2866b6aafe69fe29b1b298832db1277d411ac73c82d04d6
FROM ${RUST_IMAGE} AS rust-toolchain
FROM ${PHP_IMAGE} AS build
COPY --from=rust-toolchain /usr/local/cargo /usr/local/cargo
COPY --from=rust-toolchain /usr/local/rustup /usr/local/rustup
ENV PATH="/usr/local/cargo/bin:${PATH}" \
    CARGO_HOME=/usr/local/cargo RUSTUP_HOME=/usr/local/rustup \
    PHP=/usr/local/bin/php PHP_CONFIG=/usr/local/bin/php-config \
    CARGO_BUILD_JOBS=4
RUN apt-get update && apt-get install -y --no-install-recommends \
    $PHPIZE_DEPS clang libclang-dev git libfcgi-bin python3 \
    && rm -rf /var/lib/apt/lists/*
# PIE is a documentation/build tool, not part of the deployed PHP runtime.
RUN curl -fsSL https://github.com/php/pie/releases/download/1.5.0/pie.phar -o /usr/local/bin/pie \
    && echo 'd195dc35d311b6e0d5e2a3bd02893628f9898db3c004563f0c3a333a02010873  /usr/local/bin/pie' | sha256sum -c -
RUN rustup component add rustfmt clippy
WORKDIR /src
COPY . .
# Exercise the actual installer, not just a hand-written make approximation.
RUN php /usr/local/bin/pie repository:add path /src \
    && php /usr/local/bin/pie install 'phpstats/phit:*@dev' --no-interaction \
    && php tests/deployment.php

FROM build AS checks
RUN ./scripts/check
RUN ./scripts/check-fpm

FROM checks AS artifacts
RUN python3 scripts/package-binary.py /out

FROM scratch AS binaries
COPY --from=artifacts /out/ /

FROM ${PHP_IMAGE} AS runtime
COPY --from=checks /usr/local/lib/php/extensions/ /usr/local/lib/php/extensions/
COPY --from=checks /usr/local/etc/php/conf.d/ /usr/local/etc/php/conf.d/
RUN php -r 'if (!extension_loaded("phit") || Phit\dist_normal_cdf(0.0) !== 0.5) exit(1);'
# Supply application files in a child image or mount /var/www/html. The upstream
# PHP image retains its entrypoint and php-fpm foreground command.
