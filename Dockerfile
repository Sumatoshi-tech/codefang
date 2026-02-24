FROM golang:1.26-alpine AS builder

RUN apk add --no-cache cmake gcc g++ musl-dev linux-headers make pkgconfig

WORKDIR /src
COPY . .

# Build static binaries. STATIC=1 adds -extldflags=-static to LDFLAGS.
# Precompiled UAST mappings are checked into git, so we skip the precompile
# target (which requires a generator script not shipped in the Docker context).
RUN make libgit2

# Wrapper: add -include stdint.h only for C compilation (not assembly).
# Needed because the ansible tree-sitter grammar omits #include <stdint.h>.
RUN printf '#!/bin/sh\nfor a; do case "$a" in *.s|*.S) exec gcc "$@";; esac; done\nexec gcc -include stdint.h "$@"\n' \
    > /usr/local/bin/cc-wrap && chmod +x /usr/local/bin/cc-wrap

RUN LIBGIT2_INSTALL=third_party/libgit2/install && \
    PKG_CONFIG_PATH=$(pwd)/$LIBGIT2_INSTALL/lib64/pkgconfig:$(pwd)/$LIBGIT2_INSTALL/lib/pkgconfig \
    CGO_CFLAGS="-I$(pwd)/$LIBGIT2_INSTALL/include" \
    CGO_LDFLAGS="-L$(pwd)/$LIBGIT2_INSTALL/lib64 -L$(pwd)/$LIBGIT2_INSTALL/lib -lgit2 -lpthread" \
    CC=cc-wrap CGO_ENABLED=1 go build -ldflags '-extldflags=-static' -o build/bin/codefang ./cmd/codefang

RUN LIBGIT2_INSTALL=third_party/libgit2/install && \
    PKG_CONFIG_PATH=$(pwd)/$LIBGIT2_INSTALL/lib64/pkgconfig:$(pwd)/$LIBGIT2_INSTALL/lib/pkgconfig \
    CGO_CFLAGS="-I$(pwd)/$LIBGIT2_INSTALL/include" \
    CGO_LDFLAGS="-L$(pwd)/$LIBGIT2_INSTALL/lib64 -L$(pwd)/$LIBGIT2_INSTALL/lib -lgit2 -lpthread" \
    CC=cc-wrap CGO_ENABLED=1 go build -ldflags '-extldflags=-static' -o build/bin/uast ./cmd/uast

FROM alpine:3.21

RUN apk add --no-cache ca-certificates git bash

COPY --from=builder /src/build/bin/codefang /usr/local/bin/codefang
COPY --from=builder /src/build/bin/uast /usr/local/bin/uast
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["codefang"]
