# Arch Linux with nothing of reel's, to start the AppImage on it. The stage "clean" has no
# GStreamer, for the video and sound the AppImage carries; the last adds Weston and Arch's OBS.
FROM archlinux:base-20260927.0.600689 AS clean
RUN pacman -Syu --noconfirm --needed xorg-server-xvfb xorg-xdpyinfo mesa alsa-lib dbus ttf-dejavu xdotool imagemagick python \
    && pacman -Scc --noconfirm \
    && ! pacman -Qq | grep -E '^(gstreamer|gst-)'

FROM clean
RUN pacman -Syu --noconfirm --needed weston obs-studio \
    && pacman -Scc --noconfirm
