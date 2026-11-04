# No GStreamer in clean: the AppImage plays video and sound with what it carries.
FROM archlinux:base-20260927.0.600689 AS clean
RUN pacman -Syu --noconfirm --needed xorg-server-xvfb xorg-xdpyinfo mesa alsa-lib dbus ttf-dejavu xdotool imagemagick python \
    && pacman -Scc --noconfirm \
    && ! pacman -Qq | grep -E '^(gstreamer|gst-)'

FROM clean
RUN pacman -Syu --noconfirm --needed weston obs-studio \
    && pacman -Scc --noconfirm
