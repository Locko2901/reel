.DEFAULT_GOAL := help
.PHONY: help check check-image check-host check-text licences third-party image demo pieces-proof external-proof share-speed

CHECK_IMAGE ?= reel-check:local
TARGET_CAP_MB ?= 8000
DESKTOP_TARGET_CAP_MB ?= 8000

help: ## Show this help
	@grep -hE '^[a-zA-Z_-]+:.*?## ' $(MAKEFILE_LIST) \
	  | awk 'BEGIN{FS=":.*?## "}{printf "  \033[1m%-12s\033[0m %s\n", $$1, $$2}'

check: check-image ## Lint and tests, as CI runs them (inside the check container)
	ci/cap-volume.sh reel-target $(TARGET_CAP_MB) $(CHECK_IMAGE)
	docker run --rm \
	  -v reel-cargo-registry:/usr/local/cargo/registry \
	  -v reel-cargo-git:/usr/local/cargo/git \
	  -v reel-target:/target \
	  -e CARGO_TARGET_DIR=/target \
	  -e CARGO_INCREMENTAL=0 \
	  -v "$(CURDIR)":/work:ro \
	  -w /work \
	  $(CHECK_IMAGE) make check-host

check-image: ## Build the check container (Rust, clippy, rustfmt, ffmpeg)
	docker build -t $(CHECK_IMAGE) -f ci/check.Dockerfile ci

check-text: ## Plain text only: no dashes, special spaces, typographic quotes or pictographs
	sh ci/check-text.sh

check-host: ## The same checks with a local toolchain (needs ffmpeg for the end-to-end tests)
	sh ci/check-text.sh
	sh ci/check-demo-port.sh
	sh ci/check-obs-version.sh
	sh desktop/ci/check-commands.sh
	cargo fmt --all -- --check
	cargo clippy --all-targets -- -D warnings
	cargo clippy --all-targets --features reel/style-page -- -D warnings
	cargo test

licences: check-image ## Every crate's licence is one deny.toml allows (cargo deny, in the check container)
	docker run --rm \
	  -v reel-cargo-registry:/usr/local/cargo/registry \
	  -v reel-cargo-git:/usr/local/cargo/git \
	  -v "$(CURDIR)":/work:ro \
	  -w /work \
	  $(CHECK_IMAGE) sh ci/licences.sh

third-party: ## Write the crate list of THIRD_PARTY.md again (needs cargo and cargo-deny on the host)
	python3 ci/third-party-crates.py

external-proof: ## Link previews in an external storage against the demo and a MinIO of its own
	REEL_DEMO_PASSWORD="$(REEL_E2E_PASSWORD)" REEL_FFMPEG="$(HOME)/.local/ffmpeg/ffmpeg" ci/external-proof.sh http://127.0.0.1:8746

share-speed: ## Save and share's path on the server timed against the demo, idle and busy
	REEL_DEMO_PASSWORD="$(REEL_E2E_PASSWORD)" REEL_FFMPEG="$(HOME)/.local/ffmpeg/ffmpeg" ci/share-speed.sh http://127.0.0.1:8746

pieces-proof: ## Uploads in pieces through a proxy refusing requests over 100 MB, against the demo
	REEL_DEMO_PASSWORD="$(REEL_E2E_PASSWORD)" REEL_FFMPEG="$(HOME)/.local/ffmpeg/ffmpeg" ci/pieces-proof.sh http://127.0.0.1:8746

image: ## Build the product image locally
	docker build -t reel:local --build-arg REEL_GIT_SHA="$$(git rev-parse HEAD 2>/dev/null)" \
	  --build-arg REEL_BUILD_DATE="$$(git log -1 --format=%cs 2>/dev/null)" .

demo: ## Run the demo (scripts/demo.sh serve)
	scripts/demo.sh serve

.PHONY: docs docs-image docs-config
DOCS_IMAGE ?= reel-docs:local

docs-image:
	docker build -q -t $(DOCS_IMAGE) -f ci/docs/Dockerfile ci/docs

docs: docs-image ## Build the user site into site/, strictly (DOCS_PRIVATE_WORDS adds words it refuses)
	docker run --rm --user $(shell id -u):$(shell id -g) -e DOCS_PRIVATE_WORDS \
	  -v "$(CURDIR)":/work -w /work $(DOCS_IMAGE) mkdocs build --strict

docs-config: check-image ## Write docs/configuration.md again from the settings catalogue
	docker run --rm \
	  -v reel-cargo-registry:/usr/local/cargo/registry \
	  -v reel-cargo-git:/usr/local/cargo/git \
	  -v reel-target:/target \
	  -e CARGO_TARGET_DIR=/target \
	  -e CARGO_INCREMENTAL=0 \
	  -e UPDATE_DOCS=1 \
	  -v "$(CURDIR)":/work \
	  -w /work \
	  $(CHECK_IMAGE) sh -c 'cargo test -p reel-core --test configuration_page && chown $(shell id -u):$(shell id -g) docs/configuration.md'

# The desktop container runs as the host user, so its caches have volumes of their own: the
# server's are root-owned.
.PHONY: desktop-image desktop-cap desktop-sidecars-linux desktop-sidecars-windows desktop-check desktop-check-windows desktop-stop-helper desktop-obs-fresh desktop-elevate-check desktop-windows desktop-appimage desktop-appimage-check desktop-appimage-arch desktop-installers-check desktop-start-check desktop-obs-start-check desktop-previous desktop-obs-window-check desktop-installer-shots desktop-installer-audit desktop-obs-sources desktop-obs-image desktop-obs-e2e desktop-dev desktop-linux desktop-shell desktop-e2e desktop-shots desktop-save-actions desktop-quit desktop-waited-upload desktop-presses desktop-downloads desktop-zip-download desktop-back desktop-release-update

DESKTOP_IMAGE ?= reel-desktop:local
DESKTOP_OUT ?= $(HOME)/reel-desktop-out
DESKTOP_PREVIOUS ?= $(HOME)/reel-desktop-previous
DESKTOP_INSTALLERS ?= plain with-obs
DESKTOP_PACKAGES = $(shell sed -n 's/^ *"[a-z_]*_file": *"\([^"]*\)".*/\1/p' desktop/packages.json)
DESKTOP_VERSION ?=
DESKTOP_BUILD ?=
# An all-digit hash would be a numeric prerelease with a leading zero, which semver refuses.
DESKTOP_HASH = $(shell git rev-parse --short HEAD | sed 's/^[0-9][0-9]*$$/g&/')
DESKTOP_BASE = $(shell sed -n 's/^version = "\(.*\)"/\1/p' desktop/Cargo.toml | head -1)
REEL_E2E_SERVER ?= http://127.0.0.1:8746
REEL_E2E_USER ?= sam
REEL_E2E_PASSWORD ?= reel demo 8746
DESKTOP_SHOTS_OUT ?= shots/desktop
REEL_E2E_OBS_PASSWORD ?=
REEL_E2E_OBS_SHOTS ?=
CARGO_BUILD_JOBS ?= 4
DESKTOP_RUN = docker run --rm --network host --user $(shell id -u):$(shell id -g) \
  -v reel-desktop-cargo:/cargo-home \
  -v reel-desktop-target:/target \
  -v reel-xwin-cache:/xwin \
  -v "$(CURDIR)":/work \
  -v "$(DESKTOP_OUT)":/out \
  -v /etc/passwd:/etc/passwd:ro -v /etc/group:/etc/group:ro \
  -e CARGO_HOME=/cargo-home \
  -e CARGO_TARGET_DIR=/target \
  -e CARGO_INCREMENTAL=0 \
  -e CARGO_BUILD_JOBS=$(CARGO_BUILD_JOBS) \
  -e XDG_CACHE_HOME=/cargo-home/xdg-cache \
  -e HOME=/tmp/home

desktop-image: ## Build the desktop container (Tauri deps, cargo-xwin, NSIS, Wine, Xvfb)
	docker build -t $(DESKTOP_IMAGE) -f desktop/ci/desktop.Dockerfile desktop/ci

# Docker would make a missing bind mount as root, which the container's user cannot write.
desktop-cap: desktop-image
	mkdir -p "$(DESKTOP_OUT)"
	ci/cap-volume.sh reel-desktop-target $(DESKTOP_TARGET_CAP_MB) $(DESKTOP_IMAGE)

desktop-sidecars-linux: desktop-cap
	$(DESKTOP_RUN) -w /work -e FFMPEG_CACHE=/cargo-home/ffmpeg-cache $(DESKTOP_IMAGE) desktop/ci/fetch-ffmpeg.sh linux

desktop-sidecars-windows: desktop-cap
	$(DESKTOP_RUN) -w /work -e FFMPEG_CACHE=/cargo-home/ffmpeg-cache $(DESKTOP_IMAGE) desktop/ci/fetch-ffmpeg.sh windows

desktop-check-windows: desktop-sidecars-windows ## Clippy of the desktop workspace for Windows, cross compiled
	$(DESKTOP_RUN) -w /work/desktop $(DESKTOP_IMAGE) sh -c 'cargo xwin clippy --target x86_64-pc-windows-msvc --all-targets -- -D warnings'

desktop-stop-helper: desktop-image ## The installers' helpers and the stand-ins their checks run, built for Windows
	$(DESKTOP_RUN) -w /work/desktop $(DESKTOP_IMAGE) sh -c 'cargo xwin build --release --target x86_64-pc-windows-msvc -p reel-desktop-core --bin reel-stop --bin reel-elevate --example standin_obs --example standin_app --example elevate_parts \
	  && cp /target/x86_64-pc-windows-msvc/release/reel-stop.exe /target/x86_64-pc-windows-msvc/release/reel-elevate.exe app/nsis/ && mkdir -p .wine-standins \
	  && cp /target/x86_64-pc-windows-msvc/release/reel-stop.exe /target/x86_64-pc-windows-msvc/release/reel-elevate.exe /target/x86_64-pc-windows-msvc/release/examples/standin_obs.exe /target/x86_64-pc-windows-msvc/release/examples/standin_app.exe /target/x86_64-pc-windows-msvc/release/examples/elevate_parts.exe .wine-standins/'

desktop-obs-fresh: desktop-stop-helper ## The installers' hooks under Wine, in small installers made from them
	$(DESKTOP_RUN) -w /work $(DESKTOP_IMAGE) sh /work/desktop/ci/check-obs-fresh.sh
	$(DESKTOP_RUN) -w /work $(DESKTOP_IMAGE) sh /work/desktop/ci/check-install-stop.sh
	$(DESKTOP_RUN) -w /work $(DESKTOP_IMAGE) sh /work/desktop/ci/check-clips-guard.sh

desktop-elevate-check: desktop-stop-helper ## reel-elevate.exe under Wine, and Xvfb for the copy's OBS's window
	$(DESKTOP_RUN) -w /work $(DESKTOP_IMAGE) /work/desktop/ci/xvfb.sh run sh /work/desktop/ci/check-elevate.sh

desktop-check: desktop-sidecars-linux ## Lint and tests of the desktop workspace, inside the container
	$(DESKTOP_RUN) -w /work/desktop $(DESKTOP_IMAGE) sh -c \
	  'sh ci/check-commands.sh && sh ci/changes-test.sh && sh ci/obs-requests.sh --check && cargo fmt --all -- --check && cargo clippy --all-targets -- -D warnings && cargo test'

desktop-windows: desktop-sidecars-windows ## The Windows installers (NSIS), cross compiled; land in $(DESKTOP_OUT)
	mkdir -p "$(DESKTOP_OUT)"
	$(DESKTOP_RUN) -w /work/desktop/app -e REEL_DESKTOP_BUILD="$(DESKTOP_BUILD)" -e DESKTOP_VERSION="$(DESKTOP_VERSION)" \
	  -e REEL_INSTALLERS="$(DESKTOP_INSTALLERS)" -e OBS_CACHE=/cargo-home/obs-cache -e TAURI_SIGNING_PRIVATE_KEY \
	  -e TAURI_SIGNING_PRIVATE_KEY_PASSWORD $(DESKTOP_IMAGE) ../ci/build-windows.sh

# The GPL: a release that ships OBS carries its source.
desktop-obs-sources: desktop-image ## OBS's source archive of the pinned version into $(DESKTOP_OUT)
	mkdir -p "$(DESKTOP_OUT)"
	$(DESKTOP_RUN) -w /work -e OBS_CACHE=/cargo-home/obs-cache $(DESKTOP_IMAGE) desktop/ci/fetch-obs.sh sources /out

desktop-installers-check: desktop-stop-helper ## What both installers hold, and both installed under Wine
	$(DESKTOP_RUN) -w /work -e OBS_CACHE=/cargo-home/obs-cache $(DESKTOP_IMAGE) desktop/ci/check-installers.sh /out "$(DESKTOP_VERSION)"
	$(DESKTOP_RUN) -w /work -e OBS_CACHE=/cargo-home/obs-cache $(DESKTOP_IMAGE) sh desktop/ci/check-nsis.sh /out "$(DESKTOP_VERSION)"

desktop-obs-start-check: desktop-image ## The installer with OBS starts its OBS under Wine: fresh, without its copy, and as an update
	mkdir -p "$(DESKTOP_PREVIOUS)"
	$(DESKTOP_RUN) -w /work -v "$(DESKTOP_PREVIOUS)":/previous:ro $(DESKTOP_IMAGE) sh desktop/ci/check-obs-start.sh /out "$(DESKTOP_VERSION)" /previous

desktop-previous: desktop-image ## The newest older release's installer with OBS into $(DESKTOP_PREVIOUS), from GitHub
	mkdir -p "$(DESKTOP_PREVIOUS)"
	$(DESKTOP_RUN) -w /work -v "$(DESKTOP_PREVIOUS)":/previous -e GITHUB_TOKEN -e GITHUB_REPOSITORY $(DESKTOP_IMAGE) sh desktop/ci/fetch-previous.sh /previous "$(DESKTOP_VERSION)"

desktop-obs-window-check: desktop-stop-helper ## Open OBS and Hide OBS on the real OBS under Wine
	$(DESKTOP_RUN) -w /work -e OBS_CACHE=/cargo-home/obs-cache $(DESKTOP_IMAGE) sh desktop/ci/check-obs-window.sh

# RELEASE_PUBKEY names the .pub, in DESKTOP_OUT, of the key desktop-windows signed with when it
# is not the release key.
RELEASE_PUBKEY ?=
UPDATE_FROM ?= /out
desktop-release-update: desktop-sidecars-windows ## A standalone app's update under Wine from a stand-in release of reel light (UPDATE_FROM=github: GitHub's newest)
	$(DESKTOP_RUN) -w /work/desktop $(DESKTOP_IMAGE) sh -c 'cargo xwin test --no-run --target x86_64-pc-windows-msvc -p reel-desktop --lib'
	$(DESKTOP_RUN) -w /work $(DESKTOP_IMAGE) sh desktop/ci/check-release-update.sh $(UPDATE_FROM) "$(APP_VERSION)" $(RELEASE_PUBKEY)

desktop-start-check: desktop-stop-helper ## The app with OBS started under Wine over an older copy in Program Files
	$(DESKTOP_RUN) -w /work $(DESKTOP_IMAGE) sh desktop/ci/check-start.sh /out "$(DESKTOP_VERSION)"

INSTALLER_SHOTS ?= shots/installer
desktop-installer-shots: desktop-image ## PNGs of every installer page under Wine, into $(INSTALLER_SHOTS)
	mkdir -p "$(INSTALLER_SHOTS)"
	for f in $(DESKTOP_PACKAGES); do docker run --rm --user $(shell id -u):$(shell id -g) -v "$(CURDIR)":/work:ro \
	  -v "$(DESKTOP_OUT)":/in:ro -v "$(CURDIR)/$(INSTALLER_SHOTS)":/out $(DESKTOP_IMAGE) \
	  sh /work/desktop/ci/installer-shots.sh "/in/$${f}_$(DESKTOP_VERSION)_x64-setup.exe" /out "$$f" || exit 1; done

INSTALLER_AUDIT ?= shots/installer-audit
desktop-installer-audit: desktop-image ## PNGs of every installer page under Wine at 100, 125 and 150 %, into $(INSTALLER_AUDIT)
	for d in 96:100 120:125 144:150; do mkdir -p "$(INSTALLER_AUDIT)/$${d##*:}"; for f in $(DESKTOP_PACKAGES); do \
	  docker run --rm --user $(shell id -u):$(shell id -g) -v "$(CURDIR)":/work:ro -v "$(DESKTOP_OUT)":/in:ro \
	  -v "$(CURDIR)/$(INSTALLER_AUDIT)/$${d##*:}":/out $(DESKTOP_IMAGE) \
	  sh /work/desktop/ci/installer-shots.sh "/in/$${f}_$(DESKTOP_VERSION)_x64-setup.exe" /out "$$f" "$${d%%:*}" || exit 1; done; done

desktop-dev: ## Both Windows installers, the AppImage and the .deb, as <version>-dev.<hash>
	$(MAKE) desktop-windows DESKTOP_VERSION=$(DESKTOP_BASE)-dev.$(DESKTOP_HASH) DESKTOP_BUILD=dev.$(DESKTOP_HASH)
	$(MAKE) desktop-appimage DESKTOP_VERSION=$(DESKTOP_BASE)-dev.$(DESKTOP_HASH) DESKTOP_BUILD=dev.$(DESKTOP_HASH)

desktop-appimage: desktop-sidecars-linux ## The Linux AppImage and .deb; land in $(DESKTOP_OUT)
	mkdir -p "$(DESKTOP_OUT)"
	$(DESKTOP_RUN) -w /work/desktop/app -e REEL_DESKTOP_BUILD="$(DESKTOP_BUILD)" -e DESKTOP_VERSION="$(DESKTOP_VERSION)" \
	  -e TAURI_SIGNING_PRIVATE_KEY -e TAURI_SIGNING_PRIVATE_KEY_PASSWORD $(DESKTOP_IMAGE) ../ci/build-linux.sh

APP_VERSION = $(or $(DESKTOP_VERSION),$(DESKTOP_BASE))
ARCH_IMAGE ?= reel-arch:local
ARCH_CLEAN_IMAGE ?= reel-arch-clean:local
desktop-appimage-arch: ## The AppImage on Arch Linux: X11, Wayland, and video against the demo
	docker build --target clean -t $(ARCH_CLEAN_IMAGE) -f desktop/ci/arch.Dockerfile desktop/ci
	docker build -t $(ARCH_IMAGE) -f desktop/ci/arch.Dockerfile desktop/ci
	mkdir -p shots/linux
	docker run --rm --user $(shell id -u):$(shell id -g) -v /etc/passwd:/etc/passwd:ro -v /etc/group:/etc/group:ro \
	  -v "$(DESKTOP_OUT)":/out:ro -v "$(CURDIR)/desktop/ci":/ci:ro \
	  -v "$(CURDIR)/shots/linux":/shots $(ARCH_IMAGE) /ci/check-appimage-arch.sh /out/reel_$(APP_VERSION)_amd64.AppImage /shots
	mkdir -p "$(DESKTOP_OUT)/arch-media"
	docker run --rm --network host --user $(shell id -u):$(shell id -g) -v /etc/passwd:/etc/passwd:ro -v /etc/group:/etc/group:ro \
	  -v "$(DESKTOP_OUT)":/out:ro -v "$(CURDIR)/desktop/ci":/ci:ro -v "$(DESKTOP_OUT)/arch-media":/shots \
	  $(ARCH_CLEAN_IMAGE) /ci/check-appimage-media.sh /out/reel_$(APP_VERSION)_amd64.AppImage \
	  $(REEL_E2E_SERVER) $(REEL_E2E_USER) "$(REEL_E2E_PASSWORD)" /shots

desktop-appimage-check: desktop-image ## The AppImage and .deb checked under Xvfb, as on X11 and Wayland
	$(DESKTOP_RUN) -w /work $(DESKTOP_IMAGE) /work/desktop/ci/xvfb.sh run /work/desktop/ci/check-appimage.sh /out "$(APP_VERSION)"

desktop-linux: desktop-sidecars-linux ## A Linux debug build of the app, for the end to end run under Xvfb
	$(DESKTOP_RUN) -w /work/desktop $(DESKTOP_IMAGE) sh -c 'cargo build -p reel-desktop && cargo build -p reel-desktop-core --example fake_obs --example mic_filters'

desktop-e2e: desktop-appimage desktop-linux ## The AppImage under Xvfb against the demo on :8746 (REEL_E2E_PASSWORD)
	$(DESKTOP_RUN) -w /work -e REEL_E2E_SERVER=$(REEL_E2E_SERVER) -e REEL_E2E_USER=$(REEL_E2E_USER) \
	  -e REEL_E2E_PASSWORD="$(REEL_E2E_PASSWORD)" -e REEL_E2E_APP=/out/reel_$(APP_VERSION)_amd64.AppImage \
	  $(DESKTOP_IMAGE) /work/desktop/ci/xvfb.sh run /work/desktop/ci/e2e.sh

desktop-save-actions: desktop-linux ## Save clip and Save and share from everywhere they are, under Xvfb
	$(DESKTOP_RUN) -w /work $(DESKTOP_IMAGE) /work/desktop/ci/xvfb.sh run sh /work/desktop/ci/check-save-actions.sh

desktop-quit: desktop-linux ## The tray's Quit, each step timed, and a start right after it, under Xvfb
	$(DESKTOP_RUN) -w /work $(DESKTOP_IMAGE) /work/desktop/ci/xvfb.sh run sh /work/desktop/ci/check-quit.sh

desktop-waited-upload: desktop-linux ## An upload someone waits for goes past the pause and the queue, under Xvfb
	$(DESKTOP_RUN) -w /work -e REEL_E2E_SERVER=$(REEL_E2E_SERVER) -e REEL_E2E_USER=$(REEL_E2E_USER) -e REEL_E2E_PASSWORD="$(REEL_E2E_PASSWORD)" \
	  $(DESKTOP_IMAGE) /work/desktop/ci/xvfb.sh run sh /work/desktop/ci/check-waited-upload.sh

desktop-presses: desktop-linux ## Save and share pressed three times in a row, and a double tap, under Xvfb
	$(DESKTOP_RUN) -w /work -e REEL_E2E_SERVER=$(REEL_E2E_SERVER) -e REEL_E2E_USER=$(REEL_E2E_USER) -e REEL_E2E_PASSWORD="$(REEL_E2E_PASSWORD)" \
	  $(DESKTOP_IMAGE) /work/desktop/ci/xvfb.sh run sh /work/desktop/ci/check-presses.sh

.PHONY: desktop-notify-buttons
desktop-notify-buttons: desktop-linux ## Copy link and Open on a clip's notification, paired and standalone, under Xvfb
	$(DESKTOP_RUN) -w /work -e REEL_E2E_SERVER=$(REEL_E2E_SERVER) -e REEL_E2E_USER=$(REEL_E2E_USER) -e REEL_E2E_PASSWORD="$(REEL_E2E_PASSWORD)" \
	  $(DESKTOP_IMAGE) /work/desktop/ci/xvfb.sh run /work/desktop/ci/check-notify-buttons.sh

desktop-downloads: desktop-linux ## A download in the window says so in the page, under Xvfb
	$(DESKTOP_RUN) -w /work -e REEL_E2E_SERVER=$(REEL_E2E_SERVER) -e REEL_E2E_USER=$(REEL_E2E_USER) -e REEL_E2E_PASSWORD="$(REEL_E2E_PASSWORD)" \
	  -e REEL_SHOTS_OUT $(DESKTOP_IMAGE) /work/desktop/ci/xvfb.sh run /work/desktop/ci/check-downloads.sh

desktop-zip-download: desktop-linux ## The Select bar's Download saves its zip through the app's hook, under Xvfb
	$(DESKTOP_RUN) -w /work -e REEL_E2E_SERVER=$(REEL_E2E_SERVER) -e REEL_E2E_USER=$(REEL_E2E_USER) -e REEL_E2E_PASSWORD="$(REEL_E2E_PASSWORD)" \
	  -e REEL_SHOTS_OUT $(DESKTOP_IMAGE) /work/desktop/ci/xvfb.sh run /work/desktop/ci/check-zip-download.sh

desktop-back: desktop-linux ## The way back in the window: the button, the mouse's buttons, Alt+Left, under Xvfb
	$(DESKTOP_RUN) -w /work -e REEL_E2E_SERVER=$(REEL_E2E_SERVER) -e REEL_E2E_USER=$(REEL_E2E_USER) -e REEL_E2E_PASSWORD="$(REEL_E2E_PASSWORD)" \
	  -e REEL_SHOTS_OUT $(DESKTOP_IMAGE) /work/desktop/ci/xvfb.sh run /work/desktop/ci/check-back.sh

.PHONY: desktop-standalone
desktop-standalone: desktop-linux ## Standalone in the app under Xvfb, where a new install starts
	$(DESKTOP_RUN) -w /work -e XVFB_SCREEN=1280x900 -e REEL_SHOTS_OUT $(DESKTOP_IMAGE) /work/desktop/ci/xvfb.sh run sh /work/desktop/ci/check-standalone.sh

.PHONY: desktop-encoder-test
ENCODER_SHOTS ?= shots/encoder-test
desktop-encoder-test: desktop-linux ## The setup's encoder test in the app under Xvfb: each row as its test ends, a hung one stopped
	$(DESKTOP_RUN) -w /work -e XVFB_SCREEN=1280x900 -e REEL_SHOTS_OUT=/work/$(ENCODER_SHOTS) $(DESKTOP_IMAGE) /work/desktop/ci/xvfb.sh run sh /work/desktop/ci/check-encoder-test.sh

.PHONY: desktop-library
desktop-library: desktop-linux ## The library moved to another drive, copied, cut short and away, in the app under Xvfb
	mkdir -p "$(DESKTOP_OUT)"
	$(DESKTOP_RUN) -w /work -e XVFB_SCREEN=1280x1200 -e REEL_SHOTS_OUT $(DESKTOP_IMAGE) /work/desktop/ci/xvfb.sh run sh /work/desktop/ci/check-library.sh

desktop-shots: desktop-linux ## Screenshots of the window at 1280x800 and 720x850 against the demo (REEL_E2E_PASSWORD)
	$(DESKTOP_RUN) -w /work -e XVFB_SCREEN=1280x900 -e REEL_E2E_SERVER=$(REEL_E2E_SERVER) -e REEL_E2E_USER=$(REEL_E2E_USER) \
	  -e REEL_E2E_PASSWORD="$(REEL_E2E_PASSWORD)" -e REEL_SHOTS_OUT=/work/$(DESKTOP_SHOTS_OUT) $(DESKTOP_IMAGE) /work/desktop/ci/xvfb.sh run /work/desktop/ci/shots.sh

OBS_E2E_IMAGE ?= reel-obs-e2e:local
OBS_LINUX_URL = $(shell sed -n 's/.*"linux_url": *"\([^"]*\)".*/\1/p' desktop/obs/pinned.json)
OBS_LINUX_SHA = $(shell sed -n 's/.*"linux_sha256": *"\([^"]*\)".*/\1/p' desktop/obs/pinned.json)
OBS_UBUNTU = $(shell sed -n 's/.*"linux_ubuntu": *"\([^"]*\)".*/\1/p' desktop/obs/pinned.json)
desktop-obs-image: ## The image for the run with the real OBS
	docker build -t $(OBS_E2E_IMAGE) --build-arg UBUNTU=$(OBS_UBUNTU) --build-arg OBS_URL=$(OBS_LINUX_URL) \
	  --build-arg OBS_SHA256=$(OBS_LINUX_SHA) -f desktop/ci/obs.Dockerfile desktop/ci

desktop-obs-e2e: desktop-linux desktop-obs-image ## The Linux build with the real OBS under Xvfb
	docker run --rm --network host --user $(shell id -u):$(shell id -g) \
	  -v reel-desktop-target:/target:ro -v "$(CURDIR)":/work -v /etc/passwd:/etc/passwd:ro -v /etc/group:/etc/group:ro \
	  -e HOME=/tmp/home -e REEL_E2E_SERVER=$(REEL_E2E_SERVER) -e REEL_E2E_USER=$(REEL_E2E_USER) \
	  -e REEL_E2E_PASSWORD="$(REEL_E2E_OBS_PASSWORD)" -e REEL_E2E_SHOTS=$(REEL_E2E_OBS_SHOTS) \
	  $(OBS_E2E_IMAGE) /work/desktop/ci/xvfb.sh run /work/desktop/ci/obs-e2e.sh

desktop-shell: desktop-image ## A shell in the desktop container
	$(DESKTOP_RUN) -it -w /work/desktop $(DESKTOP_IMAGE) bash
