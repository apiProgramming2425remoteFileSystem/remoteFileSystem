# Build stage
FROM rust:slim as builder

ARG app_path=/usr/src

RUN apt-get update && apt-get install -y \
    pkg-config \
    libssl-dev \
    # FUSE 3 (Essenziale per il filesystem)
    fuse3 \
    libfuse3-dev \
    libfuse-dev \
    # GUI SLINT (Il nucleo minimo per X11)
    libx11-dev \
    libxkbcommon-x11-dev \
    libfontconfig1-dev \
    # X11 EXTENSIONS
    libxcursor-dev \
    libxrandr-dev \
    libxinerama-dev \
    libxi-dev \
    # FONT & ENGINE
    libfontconfig1-dev \
    fonts-dejavu-core \
    # Grafica (Necessaria per il rendering software/EGL)
    libgles2-mesa-dev \
    libegl1-mesa-dev

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
