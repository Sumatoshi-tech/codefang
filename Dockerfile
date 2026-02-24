FROM golang:1.26-alpine AS builder

RUN apk add --no-cache cmake gcc g++ musl-dev linux-headers make pkgconfig

WORKDIR /src
COPY . .

# Build static binaries. STATIC=1 adds -extldflags=-static to LDFLAGS.
# Precompiled UAST mappings are checked into git, so we skip the precompile
# target (which requires a generator script not shipped in the Docker context).
RUN make libgit2

RUN LIBGIT2_INSTALL=third_party/libgit2/install && \
    PKG_CONFIG_PATH=$(pwd)/$LIBGIT2_INSTALL/lib64/pkgconfig:$(pwd)/$LIBGIT2_INSTALL/lib/pkgconfig \
    CGO_CFLAGS="-I$(pwd)/$LIBGIT2_INSTALL/include -include stdint.h" \
    CGO_LDFLAGS="-L$(pwd)/$LIBGIT2_INSTALL/lib64 -L$(pwd)/$LIBGIT2_INSTALL/lib -lgit2 -lpthread" \
    CGO_ENABLED=1 go build -ldflags '-extldflags=-static' -o build/bin/codefang ./cmd/codefang

RUN LIBGIT2_INSTALL=third_party/libgit2/install && \
    PKG_CONFIG_PATH=$(pwd)/$LIBGIT2_INSTALL/lib64/pkgconfig:$(pwd)/$LIBGIT2_INSTALL/lib/pkgconfig \
    CGO_CFLAGS="-I$(pwd)/$LIBGIT2_INSTALL/include -include stdint.h" \
    CGO_LDFLAGS="-L$(pwd)/$LIBGIT2_INSTALL/lib64 -L$(pwd)/$LIBGIT2_INSTALL/lib -lgit2 -lpthread" \
    CGO_ENABLED=1 go build -ldflags '-extldflags=-static' -o build/bin/uast ./cmd/uast

FROM alpine:3.21

RUN apk add --no-cache ca-certificates git

COPY --from=builder /src/build/bin/codefang /usr/local/bin/codefang
COPY --from=builder /src/build/bin/uast /usr/local/bin/uast
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["codefang"]
