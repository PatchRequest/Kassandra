# ---- Stage 1: catalog-builder ----
# Clones TSEC CS-SA-BOF, Outflank C2-Tool-Collection, Flangvik SharpCollection at pinned
# commits, compiles BOFs with mingw-w64 + .NET tools with dotnet SDK, emits /catalog.
FROM ubuntu:22.04 AS catalog-builder

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
      git make mingw-w64 nasm \
      python3 curl ca-certificates gnupg && \
    curl -sSL https://packages.microsoft.com/config/ubuntu/22.04/packages-microsoft-prod.deb -o /tmp/ms.deb && \
    dpkg -i /tmp/ms.deb && \
    apt-get update && \
    apt-get install -y --no-install-recommends dotnet-sdk-8.0 && \
    rm -rf /var/lib/apt/lists/* /tmp/ms.deb

ARG TSEC_REF=ee9459cc4f42c6b025797bad22ffe8d9f1cf6487
ARG OUTFLANK_REF=e371a38c717edaf1650923575ab33bee0dd3e0ee
ARG SHARP_REF=dad01b93abf5074e72b94218b74bee310f6eb74a

WORKDIR /src
RUN git clone https://github.com/trustedsec/CS-Situational-Awareness-BOF.git tsec && \
    git -C tsec checkout ${TSEC_REF}
RUN git clone https://github.com/outflanknl/C2-Tool-Collection.git outflank && \
    git -C outflank checkout ${OUTFLANK_REF}
RUN git clone https://github.com/Flangvik/SharpCollection.git sharp && \
    git -C sharp checkout ${SHARP_REF}

COPY build_catalog.sh /tmp/build_catalog.sh
RUN chmod +x /tmp/build_catalog.sh && /tmp/build_catalog.sh


# ---- Stage 2: runtime ----
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
      build-essential curl zip nasm git \
      libssl-dev zlib1g-dev libbz2-dev \
      xz-utils tk-dev libffi-dev liblzma-dev \
      libsqlite3-dev protobuf-compiler \
      software-properties-common apt-utils \
      mingw-w64 && \
    rm -rf /var/lib/apt/lists/*

COPY --from=catalog-builder /catalog /opt/kassandra_catalog

# BinaryFiller: corpus (ops fail policy) + source for prebuilding the stamp-cert CLI.
# Bundled goodware PEs under corpus/bundled/ are Authenticode donors for post-link cert clone.
ARG BINARY_FILLER_REF=main
RUN git clone --depth 1 --branch "${BINARY_FILLER_REF}" \
      https://github.com/PatchRequest/BinaryFiller.git /opt/BinaryFiller && \
    mkdir -p /opt/bf-corpus && \
    cp -a /opt/BinaryFiller/corpus/. /opt/bf-corpus/
ENV BINARY_FILLER_CORPUS=/opt/bf-corpus

ENV PATH="/root/.cargo/bin:/usr/local/go/bin:${PATH}"
ENV CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc

# Install Go (needed to build tailscale FFI library)
RUN curl -fsSL https://go.dev/dl/go1.24.2.linux-amd64.tar.gz | tar -C /usr/local -xzf -

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain nightly-2025-04-30 && \
    rustup +nightly-2025-04-30 target add x86_64-pc-windows-gnu

# Pre-build BinaryFiller CLI (stamp-cert / verify). Replaces osslsigncode self-sign path.
# Host (Linux) binary — runs inside the payload build container, not on the implant.
RUN cd /opt/BinaryFiller && \
    cargo +nightly-2025-04-30 install --path crates/binary-filler-cli --root /usr/local --force && \
    test -x /usr/local/bin/binary-filler && \
    rm -rf /opt/BinaryFiller

COPY requirements.txt /
RUN pip3 install -r /requirements.txt

# Pre-build loader DLLs (BOF + .NET) and encrypt them for on-demand delivery
COPY Kassandra/agent_code/kassandra/loaders /tmp/loaders
RUN cd /tmp/loaders/bof-loader && \
    cargo +nightly-2025-04-30 build --release --target x86_64-pc-windows-gnu && \
    cd /tmp/loaders/dot-loader && \
    cargo +nightly-2025-04-30 build --release --target x86_64-pc-windows-gnu && \
    mkdir -p /opt/loaders && \
    cp /tmp/loaders/bof-loader/target/x86_64-pc-windows-gnu/release/bof_loader.dll /opt/loaders/bof_loader.dll && \
    cp /tmp/loaders/dot-loader/target/x86_64-pc-windows-gnu/release/dot_loader.dll /opt/loaders/dot_loader.dll && \
    rm -rf /tmp/loaders

# Pre-build the Tailscale FFI static library for Windows (cross-compiled via Go + mingw)
COPY Kassandra/agent_code/kassandra/tailscale_ffi /tmp/tailscale_ffi
RUN cd /tmp/tailscale_ffi && \
    go mod tidy && \
    CGO_ENABLED=1 CC=x86_64-w64-mingw32-gcc GOOS=windows GOARCH=amd64 \
      go build -buildmode=c-archive -ldflags="-s -w" -o /opt/tailscale_ffi/libtailscale_ffi.a . && \
    rm -rf /tmp/tailscale_ffi

WORKDIR /Mythic/
CMD ["python3", "main.py"]
