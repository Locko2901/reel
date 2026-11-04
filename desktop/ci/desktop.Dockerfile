FROM rust:1-bookworm
ARG TAURI_CLI=2.12.1
ARG CARGO_XWIN=0.23.1
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        build-essential pkg-config file make git curl wget ca-certificates \
        libwebkit2gtk-4.1-dev libgtk-3-dev libayatana-appindicator3-dev \
        librsvg2-dev librsvg2-bin libsoup-3.0-dev libjavascriptcoregtk-4.1-dev patchelf \
        clang lld llvm \
        xvfb xauth x11-utils xdotool imagemagick trayer dbus dbus-x11 \
        pulseaudio pulseaudio-utils alsa-utils \
        ffmpeg fonts-dejavu-core \
    && rm -rf /var/lib/apt/lists/*
RUN rustup component add rustfmt clippy \
    && rustup target add x86_64-pc-windows-msvc
RUN curl -fsSL "https://github.com/tauri-apps/tauri/releases/download/tauri-cli-v${TAURI_CLI}/cargo-tauri-x86_64-unknown-linux-gnu.tgz" \
        | tar -xz -C /usr/local/cargo/bin \
    && curl -fsSL "https://github.com/rust-cross/cargo-xwin/releases/download/v${CARGO_XWIN}/cargo-xwin-v${CARGO_XWIN}.x86_64-unknown-linux-musl.tar.gz" \
        | tar -xz -C /usr/local/cargo/bin \
    && chmod 755 /usr/local/cargo/bin/cargo-tauri /usr/local/cargo/bin/cargo-xwin \
    && cargo tauri --version && cargo xwin --version
# Named volumes inherit this mode on their first mount, so the host user can write them.
RUN mkdir -m 1777 /cargo-home /target /xwin /tmp/home /out
RUN apt-get update && apt-get install -y --no-install-recommends unzip && rm -rf /var/lib/apt/lists/*
# Accepts Microsoft's licence of the CRT and Windows SDK cargo-xwin downloads.
ENV XWIN_ACCEPT_LICENSE=1 \
    XWIN_CACHE_DIR=/xwin \
    CARGO_TERM_COLOR=always \
    REEL_FFMPEG=ffmpeg \
    REEL_FFPROBE=ffprobe
WORKDIR /work/desktop
# Debian's Wine 8.0 lacks bcryptprimitives.dll, which Rust's standard library imports on Windows.
ARG WINE_VERSION=11.0.0.0~bookworm-1
ARG WINEHQ_FINGERPRINT=D43F640145369C51D786DDEA76F1A20FF987672F
RUN dpkg --add-architecture i386 && mkdir -p /etc/apt/keyrings \
    && curl -fsSL -o /tmp/winehq.key https://dl.winehq.org/wine-builds/winehq.key \
    && test "$(gpg --show-keys --with-colons /tmp/winehq.key | awk -F: '/^fpr/ {print $10; exit}')" = "${WINEHQ_FINGERPRINT}" \
    && gpg --dearmor -o /etc/apt/keyrings/winehq.gpg /tmp/winehq.key && rm /tmp/winehq.key \
    && echo "deb [signed-by=/etc/apt/keyrings/winehq.gpg] https://dl.winehq.org/wine-builds/debian bookworm main" > /etc/apt/sources.list.d/winehq.list \
    && apt-get update \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        winehq-stable=${WINE_VERSION} wine-stable=${WINE_VERSION} wine-stable-amd64=${WINE_VERSION} wine-stable-i386:i386=${WINE_VERSION} \
    && rm -rf /var/lib/apt/lists/* && wine --version
# For the AppImage check's stand-in GlobalShortcuts portal.
RUN apt-get update && apt-get install -y --no-install-recommends python3-dbus python3-gi gir1.2-glib-2.0 \
    && rm -rf /var/lib/apt/lists/*
# What the AppImage carries: H.264 and AAC, ALSA without a sound server, GL for WebKitGTK's video.
RUN apt-get update && apt-get install -y --no-install-recommends gstreamer1.0-libav gstreamer1.0-alsa gstreamer1.0-gl \
    && rm -rf /var/lib/apt/lists/*
# Debian's NSIS 3.08 lacks Win\RestartManager.nsh, which Tauri's script includes (the same to 3.11).
ARG NSIS_RM_SHA256=67149978412da9b283e2e1abe48a360093df9caf6363373e2cabe463978b7c0e
RUN apt-get update && apt-get install -y --no-install-recommends nsis p7zip-full \
    && rm -rf /var/lib/apt/lists/* \
    && curl -fsSL -o /usr/share/nsis/Include/Win/RestartManager.nsh \
        "https://raw.githubusercontent.com/kichik/nsis/v311/Include/Win/RestartManager.nsh" \
    && echo "${NSIS_RM_SHA256}  /usr/share/nsis/Include/Win/RestartManager.nsh" | sha256sum -c -
