# syntax=docker/dockerfile:1.7
FROM rust:1-bookworm AS build
WORKDIR /app
ARG CARGO_BUILD_JOBS=default
ARG REEL_GIT_SHA=
ARG REEL_BUILD_DATE=
ENV REEL_GIT_SHA=$REEL_GIT_SHA REEL_BUILD_DATE=$REEL_BUILD_DATE
COPY Cargo.toml Cargo.lock CHANGELOG.md ./
COPY crates ./crates
COPY web ./web
# The OBS the app's installer carries, named on the Status page next to ffmpeg, and the names
# of the app's Windows downloads.
COPY desktop/obs/pinned.json ./desktop/obs/pinned.json
COPY desktop/packages.json ./desktop/packages.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked -p reel \
    && cp target/release/reel /usr/local/bin/reel

# jellyfin-ffmpeg has QSV and VAAPI built in; the Intel media driver gives it an Intel iGPU.
FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates curl gnupg tzdata \
    && curl -fsSL https://repo.jellyfin.org/jellyfin_team.gpg.key | gpg --dearmor -o /usr/share/keyrings/jellyfin.gpg \
    && echo "deb [signed-by=/usr/share/keyrings/jellyfin.gpg arch=amd64] https://repo.jellyfin.org/debian bookworm main" > /etc/apt/sources.list.d/jellyfin.list \
    && sed -i 's/^Components: main$/Components: main non-free non-free-firmware/' /etc/apt/sources.list.d/debian.sources \
    && apt-get update \
    && apt-get install -y --no-install-recommends jellyfin-ffmpeg7 intel-media-va-driver-non-free libigdgmm12 libva2 libva-drm2 gosu \
    && apt-get purge -y gnupg && apt-get autoremove -y && rm -rf /var/lib/apt/lists/* \
    && useradd -u 1000 -d /data -M reel \
    && mkdir -p /data /incoming /originals && chown reel:reel /data /incoming /originals
COPY --from=build /usr/local/bin/reel /usr/local/bin/reel
COPY deploy/entrypoint.sh /usr/local/bin/entrypoint.sh
COPY deploy/config.example.toml /usr/share/reel/config.example.toml
ENV REEL_CONFIG=/data/config.toml \
    REEL_LISTEN=0.0.0.0:8080 \
    REEL_DATA_DIR=/data \
    REEL_INCOMING_DIR=/incoming \
    REEL_ORIGINALS_DIR=/originals \
    REEL_FFMPEG=/usr/lib/jellyfin-ffmpeg/ffmpeg \
    REEL_FFPROBE=/usr/lib/jellyfin-ffmpeg/ffprobe \
    LIBVA_DRIVER_NAME=iHD \
    TZ=UTC
VOLUME ["/data", "/incoming", "/originals"]
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=5s --start-period=15s \
    CMD curl -fsS http://127.0.0.1:8080/healthz >/dev/null || exit 1
# Starts as root only to adopt the render device's group, then drops to the reel user.
ENTRYPOINT ["/usr/local/bin/entrypoint.sh"]
CMD ["serve"]
