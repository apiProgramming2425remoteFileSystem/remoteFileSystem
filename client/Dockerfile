# Build stage
FROM rust:slim as builder

ARG app_path=/usr/src

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    fuse3 \
    libfuse3-dev \
    libfuse-dev 

WORKDIR ${app_path}/app
COPY . .

RUN cargo build --release

# Runtime stage
FROM ubuntu:latest as runtime

ARG app_path=/usr/src
ARG remote_fs_client=remote_fs_client

RUN apt-get update && apt-get install -y \
    libx11-dev \
    libxkbcommon-x11-dev \
    libfontconfig1-dev \
    libxcursor-dev \
    libxrandr-dev \
    libxinerama-dev \
    libxi-dev \
    libfontconfig1-dev \
    fonts-dejavu-core \
    libgles2-mesa-dev \
    libegl1-mesa-dev

COPY --from=builder ${app_path}/app/target/release/${remote_fs_client} /usr/local/bin/remote_fs_client

# Entrypoint runs your daemon binary
ENTRYPOINT ["/usr/local/bin/remote_fs_client"]
