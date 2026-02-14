# Stage 1: Build codefang and uast binaries with CGO dependencies.
FROM golang:1.26-bookworm AS builder

# Install build dependencies for libgit2 and tree-sitter.
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    cmake \
    libssl-dev \
    libz-dev \
    pkg-config \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /src

# Copy dependency files first for layer caching.
COPY go.mod go.sum ./
RUN go mod download && go mod verify

# Copy vendored libgit2 source and build it statically.
COPY third_party/ third_party/
RUN mkdir -p third_party/libgit2/build && \
    cd third_party/libgit2/build && \
    cmake .. \
    -DCMAKE_INSTALL_PREFIX=/src/third_party/libgit2/install \
    -DCMAKE_BUILD_TYPE=Release \
    -DBUILD_SHARED_LIBS=OFF \
    -DBUILD_TESTS=OFF \
    -DBUILD_CLI=OFF \
    -DUSE_SSH=OFF \
    -DUSE_HTTPS=OpenSSL && \
    cmake --build . --parallel && \
    cmake --install .

# Copy the rest of the source code.
COPY . .

# Set CGO environment for libgit2.
ENV PKG_CONFIG_PATH=/src/third_party/libgit2/install/lib64/pkgconfig:/src/third_party/libgit2/install/lib/pkgconfig
ENV CGO_CFLAGS="-I/src/third_party/libgit2/install/include"
ENV CGO_LDFLAGS="-L/src/third_party/libgit2/install/lib64 -L/src/third_party/libgit2/install/lib -lgit2 -lz -lssl -lcrypto -lpthread"
ENV CGO_ENABLED=1

# Pre-compile UAST mappings.
RUN go run ./build/scripts/precompgen/precompile.go -o pkg/uast/embedded_mappings.gen.go

# Build both binaries with version info.
ARG VERSION=dev
ARG COMMIT=none
ARG BUILD_DATE=unknown

RUN go build \
    -ldflags "-X github.com/Sumatoshi-tech/codefang/pkg/version.Version=${VERSION} \
    -X github.com/Sumatoshi-tech/codefang/pkg/version.Commit=${COMMIT} \
    -X github.com/Sumatoshi-tech/codefang/pkg/version.Date=${BUILD_DATE} \
    -s -w" \
    -o /out/codefang ./cmd/codefang

RUN go build \
    -ldflags "-s -w" \
    -o /out/uast ./cmd/uast

# Verify binaries are functional.
RUN /out/codefang --help > /dev/null 2>&1
RUN /out/uast --help > /dev/null 2>&1

# Stage 2: Minimal runtime image.
# Using debian:bookworm-slim (not alpine) for glibc compatibility with CGO binaries.
FROM debian:bookworm-slim AS runtime

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
    ca-certificates \
    git \
    libgomp1 \
    libssl3 \
    zlib1g \
    && rm -rf /var/lib/apt/lists/*

# Copy binaries from builder.
# libgit2 is statically linked; only system libs (ssl, zlib) needed at runtime.
COPY --from=builder /out/codefang /usr/local/bin/codefang
COPY --from=builder /out/uast /usr/local/bin/uast

# Copy entrypoint for GitHub Action usage.
COPY build/package/entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

# Create non-root user for security.
RUN useradd --create-home --shell /bin/bash codefang
USER codefang
WORKDIR /workspace

ENTRYPOINT ["codefang"]
CMD ["--help"]
