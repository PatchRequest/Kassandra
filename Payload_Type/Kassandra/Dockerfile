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

ENV PATH="/root/.cargo/bin:${PATH}"
ENV CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc
ENV RUSTFLAGS="-C codegen-units=16"

# Install only the nightly toolchain we actually use
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain nightly-2025-04-30 && \
    rustup +nightly-2025-04-30 target add x86_64-pc-windows-gnu

COPY requirements.txt /
RUN pip3 install -r /requirements.txt

# Pre-generate the self-signed code signing certificate (reused across builds)
RUN openssl genrsa -out /opt/codesign.key 2048 && \
    openssl req -new -x509 -key /opt/codesign.key -out /opt/codesign.crt -days 3650 -subj "/CN=SAP/O=HANA" && \
    openssl pkcs12 -export -out /opt/codesign.pfx -inkey /opt/codesign.key -in /opt/codesign.crt -passout pass:infected

# Pre-compile all Rust dependencies into the cargo registry/git cache.
# Only Cargo.toml, Cargo.lock, build.rs, and the asm stub are needed.
# A dummy main.rs lets cargo resolve the full dependency graph.
# The compiled deps land in the shared cargo cache (~/.cargo + target/).
COPY Kassandra/agent_code/kassandra/Cargo.toml Kassandra/agent_code/kassandra/Cargo.lock /tmp/depbuild/
COPY Kassandra/agent_code/kassandra/build.rs /tmp/depbuild/
COPY Kassandra/agent_code/kassandra/src/asm/hellsasm.asm /tmp/depbuild/src/asm/
RUN echo 'fn main() {}' > /tmp/depbuild/src/main.rs && \
    cd /tmp/depbuild && \
    cargo +nightly-2025-04-30 build --release --target x86_64-pc-windows-gnu 2>&1 || true && \
    rm -rf /tmp/depbuild

WORKDIR /Mythic/
CMD ["python3", "main.py"]
