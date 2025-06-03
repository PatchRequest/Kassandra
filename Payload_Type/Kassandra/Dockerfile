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

RUN curl https://sh.rustup.rs -sSf | sh -s -- -y --default-toolchain nightly-2025-05-21 
    

COPY requirements.txt /
RUN pip3 install -r /requirements.txt

RUN rustup install nightly-2025-04-30
RUN rustup override set nightly-2025-04-30
RUN rustup +nightly-2025-04-30 target add x86_64-pc-windows-gnu

WORKDIR /Mythic/
CMD ["python3", "main.py"]
