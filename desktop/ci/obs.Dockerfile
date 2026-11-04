# check=skip=InvalidDefaultArgInFrom
# The app with the real OBS under Xvfb. OBS packages only its newest Ubuntu, named in the pin as
# linux_ubuntu and passed as UBUNTU with no default here, hence the check skipped above.
ARG UBUNTU
FROM ubuntu:${UBUNTU}
ARG OBS_URL
ARG OBS_SHA256
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates curl \
    && curl -fsSL --retry 3 -o /tmp/obs.deb "$OBS_URL" \
    && echo "$OBS_SHA256  /tmp/obs.deb" | sha256sum -c - \
    && apt-get install -y --no-install-recommends /tmp/obs.deb \
        libwebkit2gtk-4.1-0 libayatana-appindicator3-1 librsvg2-common libgl1-mesa-dri \
        xvfb xauth x11-utils xdotool trayer dbus dbus-x11 pulseaudio pulseaudio-utils \
        ffmpeg python3 imagemagick fonts-dejavu-core \
    && rm -f /tmp/obs.deb && rm -rf /var/lib/apt/lists/* \
    && obs --version && test -x /usr/local/bin/obs
RUN mkdir -m 1777 /tmp/home /out
ENV REEL_FFMPEG=ffmpeg \
    REEL_FFPROBE=ffprobe
