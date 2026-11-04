# The container `make check` runs in; the font is for the synthetic clips' drawtext.
FROM rust:1-bookworm
RUN apt-get update \
    && apt-get install -y --no-install-recommends ffmpeg fonts-dejavu-core make \
    && rm -rf /var/lib/apt/lists/* \
    && rustup component add rustfmt clippy
ENV REEL_FFMPEG=ffmpeg \
    REEL_FFPROBE=ffprobe \
    CARGO_TERM_COLOR=always
WORKDIR /work
