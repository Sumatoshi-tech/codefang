FROM golang:1.24-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    cmake libssl-dev libz-dev && \
    rm -rf /var/lib/apt/lists/*

WORKDIR /src
COPY . .

RUN make libgit2
RUN make build

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 zlib1g git && \
    rm -rf /var/lib/apt/lists/*

COPY --from=builder /src/build/bin/codefang /usr/local/bin/codefang
COPY --from=builder /src/build/bin/uast /usr/local/bin/uast
COPY entrypoint.sh /entrypoint.sh
RUN chmod +x /entrypoint.sh

ENTRYPOINT ["codefang"]
