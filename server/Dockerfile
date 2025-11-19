# Build stage
FROM rust:slim as builder

ARG app_path=/usr/src

WORKDIR ${app_path}/server
COPY . .

RUN cargo build --release

# Runtime stage
FROM ubuntu:latest as runtime

ARG app_path=/usr/src
ARG remote_fs_server=remote_fs_server

COPY --from=builder ${app_path}/server/target/release/${remote_fs_server} /usr/local/bin/remote_fs_server

EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/remote_fs_server"]
