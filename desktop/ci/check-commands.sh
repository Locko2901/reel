#!/bin/sh
# Tests and build scripts run on the build machine only.
set -eu
cd "$(dirname "$0")/../.."
found=$(grep -rnE 'Command::new|\.creation_flags\(' --include='*.rs' crates/*/src desktop/*/src | grep -v '^crates/core/src/proc\.rs:' || true)
if [ -n "$found" ]; then
  echo "a process started outside reel_core::proc; on Windows it would flash a console window:"
  echo "$found"
  exit 1
fi
echo "every process goes through reel_core::proc"

# Tauri refuses a command without a permission, so a button calling it would do nothing.
handler=$(sed -n '/generate_handler!\[/,/\]/p' desktop/app/src/lib.rs | grep -oE '^ +[a-z_]+::[a-z_]+,' | sed 's/.*:://; s/,//' | sort)
listed=$(sed -n '/^const COMMANDS/,/^];/p' desktop/app/build.rs | grep -o '"[a-z_]*"' | tr -d '"' | sort)
granted=$(grep -o '"allow-[a-z-]*"' desktop/app/capabilities/default.json | tr -d '"' | sed 's/^allow-//; s/-/_/g' | sort)
remote=$(grep -o '"allow-[a-z-]*"' desktop/app/src/shell.rs | tr -d '"' | sed 's/^allow-//; s/-/_/g' | sort)
missing=$(printf '%s\n' "$handler" | grep -vxF "$listed" || true)
stale=$(printf '%s\n' "$listed" | grep -vxF "$handler" || true)
ungranted=$(printf '%s\n' "$listed" | grep -vxF "$granted" | grep -vxF "$remote" || true)
if [ -n "$missing$stale$ungranted" ]; then
  [ -z "$missing" ] || echo "registered without a permission in build.rs: $missing"
  [ -z "$stale" ] || echo "in build.rs but not registered: $stale"
  [ -z "$ungranted" ] || echo "granted to no page: $ungranted"
  exit 1
fi
echo "every command of the app has its permission and a page it is granted to"
