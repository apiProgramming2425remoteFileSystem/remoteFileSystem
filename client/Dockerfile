# Build stage
FROM rust:slim as builder

ARG app_path=/usr/src

RUN apt-get update && apt-get install -y fuse3 libfuse3-dev libfuse-dev pkg-config libssl-dev

WORKDIR ${app_path}/app
COPY . .

RUN cargo build --release

# Runtime stage
FROM ubuntu:latest as runtime

ARG app_path=/usr/src
ARG remote_fs_client=remote_fs_client

COPY --from=builder ${app_path}/app/target/release/${remote_fs_client} /usr/local/bin/remote_fs_client

# Entrypoint runs your daemon binary
ENTRYPOINT ["/usr/local/bin/remote_fs_client"]
