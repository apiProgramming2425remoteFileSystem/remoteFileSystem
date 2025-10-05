# Build stage
FROM rust:slim as builder

ARG app_path=/usr/src

WORKDIR ${app_path}/backend
COPY . .

RUN cargo build --release

# Runtime stage
FROM ubuntu:latest

ARG app_path=/usr/src
ARG remote_fs_backend=remote_fs_backend

COPY --from=builder ${app_path}/backend/target/release/${remote_fs_backend} /usr/local/bin/remote_fs_backend

EXPOSE 8080

ENTRYPOINT ["/usr/local/bin/remote_fs_backend"]
