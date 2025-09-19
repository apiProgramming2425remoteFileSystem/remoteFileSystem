# Build stage
FROM rust:slim as builder

ARG app_path=/usr/src

WORKDIR ${app_path}/app
COPY . .


RUN cargo clean && cargo build --release

# Runtime stage
FROM ubuntu:latest

ARG app_path=/usr/src
ARG remote_fs_client=remote_fs_client

# RUN apt-get update && apt-get install -y fuse libfuse3-dev
RUN apt-get update && apt-get install -y fuse3 libfuse3-dev libfuse-dev pkg-config libssl-dev


COPY --from=builder ${app_path}/app/target/release/${remote_fs_client} /usr/local/bin/remote_fs_client

# Entrypoint runs your daemon binary
ENTRYPOINT ["/usr/local/bin/remote_fs_client"]
