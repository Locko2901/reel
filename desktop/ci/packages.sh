# The two Windows packages as desktop/packages.json names them, for scripts to source: the
# installers' files before _<version>_x64-setup.exe, and the names people read.
packages_json="${PACKAGES_JSON:-/work/desktop/packages.json}"
package() { sed -n "s/^ *\"$1\": *\"\([^\"]*\)\".*/\1/p" "$packages_json"; }
WITH_OBS_NAME=$(package with_obs_name) WITH_OBS_FILE=$(package with_obs_file)
PLAIN_NAME=$(package plain_name) PLAIN_FILE=$(package plain_file)
[ -n "$WITH_OBS_FILE" ] && [ -n "$PLAIN_FILE" ] || { echo "packages.sh: $packages_json names no packages" >&2; exit 1; }
