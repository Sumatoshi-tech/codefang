FROM golang:1.26-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake libssl-dev libz-dev && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .

RUN make libgit2

# Build binaries directly — precompiled UAST mappings are checked into git,
# so we skip the precompile target (which requires a generator script not
# shipped in the Docker context).
RUN LIBGIT2_INSTALL=third_party/libgit2/install && \
    PKG_CONFIG_PATH=$(pwd)/$LIBGIT2_INSTALL/lib64/pkgconfig:$(pwd)/$LIBGIT2_INSTALL/lib/pkgconfig \
    CGO_CFLAGS="-I$(pwd)/$LIBGIT2_INSTALL/include" \
    CGO_LDFLAGS="-L$(pwd)/$LIBGIT2_INSTALL/lib64 -L$(pwd)/$LIBGIT2_INSTALL/lib -lgit2 -lz -lssl -lcrypto -lpthread" \
    CGO_ENABLED=1 go build -o build/bin/codefang ./cmd/codefang

RUN LIBGIT2_INSTALL=third_party/libgit2/install && \
    PKG_CONFIG_PATH=$(pwd)/$LIBGIT2_INSTALL/lib64/pkgconfig:$(pwd)/$LIBGIT2_INSTALL/lib/pkgconfig \
    CGO_CFLAGS="-I$(pwd)/$LIBGIT2_INSTALL/include" \
    CGO_LDFLAGS="-L$(pwd)/$LIBGIT2_INSTALL/lib64 -L$(pwd)/$LIBGIT2_INSTALL/lib -lgit2 -lz -lssl -lcrypto -lpthread" \
    CGO_ENABLED=1 go build -o build/bin/uast ./cmd/uast

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 zlib1g git && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/build/bin/codefang /usr/local/bin/codefang
COPY --from=builder /src/build/bin/uast /usr/local/bin/uast
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["codefang"]
