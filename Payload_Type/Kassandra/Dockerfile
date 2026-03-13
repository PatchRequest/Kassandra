FROM ubuntu:22.04

ARG CA_CERTIFICATE
ARG NPM_REGISTRY
ARG PYPI_INDEX
ARG PYPI_INDEX_URL
ARG DOCKER_REGISTRY_MIRROR
ARG HTTP_PROXY
ARG HTTPS_PROXY

ENV DEBIAN_FRONTEND=noninteractive


# combine update+install into one RUN to keep indices fresh
RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      python3 python3-pip python3-venv \
      build-essential curl zip nasm \
      libssl-dev zlib1g-dev libbz2-dev \
      xz-utils tk-dev libffi-dev liblzma-dev \
      libsqlite3-dev protobuf-compiler \
      software-properties-common apt-utils \
      mingw-w64 \
      osslsigncode openssl && \
    rm -rf /var/lib/apt/lists/*


ENV PATH="/root/.cargo/bin:/usr/local/go/bin:${PATH}"
ENV CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc

# Install Go (needed to build tailscale FFI library)
RUN curl -fsSL https://go.dev/dl/go1.24.2.linux-amd64.tar.gz | tar -C /usr/local -xzf -

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain nightly-2025-05-21


COPY requirements.txt /
RUN pip3 install -r /requirements.txt

RUN rustup install nightly-2025-04-30
RUN rustup override set nightly-2025-04-30
RUN rustup +nightly-2025-04-30 target add x86_64-pc-windows-gnu

# Pre-build the Tailscale FFI static library for Windows (cross-compiled via Go + mingw)
COPY Kassandra/agent_code/kassandra/tailscale_ffi /tmp/tailscale_ffi
RUN cd /tmp/tailscale_ffi && \
    go mod tidy && \
    CGO_ENABLED=1 CC=x86_64-w64-mingw32-gcc GOOS=windows GOARCH=amd64 \
      go build -buildmode=c-archive -ldflags="-s -w" -o /opt/tailscale_ffi/libtailscale_ffi.a . && \
    rm -rf /tmp/tailscale_ffi

WORKDIR /Mythic/
CMD ["python3", "main.py"]
