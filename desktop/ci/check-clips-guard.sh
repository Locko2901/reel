#!/bin/sh
# The uninstaller's clips folder under Wine, reel-stop.exe on real folders, links and junctions:
# the page's answer for each, and a removal that never touches a folder the answer keeps.
set -eu
H=/work/desktop/app/nsis/reel-stop.exe
W=$(mktemp -d)
export HOME="$W/home" WINEPREFIX="$W/wine" WINEDEBUG=-all WINEDLLOVERRIDES="mscoree,mshtml="
mkdir -p "$HOME"
fail() { printf 'FAIL %s\n' "$*"; exit 1; }
ok() { printf 'ok   %s\n' "$*"; }
wineboot -i >/dev/null 2>&1
wineserver -w
L=$(wine cmd /c "echo %LOCALAPPDATA%" 2>/dev/null | tr -d '\r')
R=$(wine cmd /c "echo %APPDATA%" 2>/dev/null | tr -d '\r')
U=$(wine cmd /c "echo %USERPROFILE%" 2>/dev/null | tr -d '\r')
I="$L\\reel"
SET="$(winepath -u "$R")/reel"
C=$(winepath -u 'C:\')
mkdir -p "$SET" "$(winepath -u "$I")" "$C/Clips/Valorant" "$C/Elsewhere" "$C/Real"
head -c 3000 /dev/urandom > "$C/Clips/Valorant/Replay 1.mp4"
head -c 2000 /dev/urandom > "$C/Clips/Replay 2.mkv"
head -c 9000 /dev/urandom > "$C/Elsewhere/not the clips.txt"
link() { wine cmd /c "mklink $1 $2 $3" >/dev/null 2>&1 || fail "mklink $*"; }
link /j 'C:\Clips\Elsewhere' 'C:\Elsewhere'
link /j 'C:\Junc' 'C:\Real'
link /d 'C:\Sym' 'C:\Real'
link /j 'C:\Into' 'C:\windows'

answer() { # answer <clips_dir as the settings write it>: the page's line
  printf '{\n  "clips_dir": "%s"\n}' "$1" > "$SET/settings.json"
  rm -f "$C/answer.txt"
  wine "$H" --clips 'C:\answer.txt' "$I" >/dev/null 2>&1 || fail "--clips $1: exit $?"
  iconv -f UTF-16 -t UTF-8 "$C/answer.txt"
}
want() { # want <clips_dir> <line>
  got=$(answer "$1")
  [ "$got" = "$2" ] || fail "$1: \"$got\", not \"$2\""
}
want 'C:\\Clips' 'offer|C:\Clips|5 KB, 2 files'
want 'C:/Clips/' 'offer|C:\Clips|5 KB, 2 files'
want 'C:\\Nothing' 'missing'
want 'C:\\' 'keep|C:\|it is a whole drive'
want 'Clips' 'keep|Clips|its path does not start from a drive letter, such as D:\'
want '\\\\nas\\share\\Clips' 'keep|\\nas\share\Clips|it is a folder on the network'
want 'C:\\Clips\\..\\users' 'keep|C:\Clips\..\users|its path has . or .. in it'
want 'C:\\Junc' 'keep|C:\Junc|it is a link or a mount point, not a folder of its own'
want 'C:\\Sym' 'keep|C:\Sym|it is a link or a mount point, not a folder of its own'
want 'C:\\Into\\system32' 'keep|C:\Into\system32|it is inside Windows'
want 'C:\\users' 'keep|C:\users|it holds your user folder'
want "$(printf '%s' "$U" | sed 's/\\/\\\\/g')" "keep|$U|it is your user folder"
want 'C:\\windows' 'keep|C:\windows|it is Windows'
want 'C:\\Program Files (x86)' 'keep|C:\Program Files (x86)|it is Program Files (x86)'
want 'C:\\ProgramData' 'keep|C:\ProgramData|it is ProgramData'
want "$(printf '%s' "$L" | sed 's/\\/\\\\/g')" "keep|$L|it is your local app data"
want "$(printf '%s' "$I" | sed 's/\\/\\\\/g')" "keep|$I|it is reel's install folder"
want "$(printf '%s' "$R\\reel" | sed 's/\\/\\\\/g')" "keep|$R\\reel|it is reel's settings"
ok "the page's answer: C:\\Clips offered at its own size, the junction in it not counted; 16 others kept or not there, each with why"

drop() { # drop <folder>: the removal's line, and its exit
  if wine "$H" --delete-clips "$1" "$I" >"$W/drop" 2>/dev/null; then s=0; else s=$?; fi
  tr -d '\r' <"$W/drop"
  return $s
}
out=$(drop 'C:\users') && fail "C:\\users was deleted: $out"
[ -d "$C/users" ] && [ "$out" = 'the clips folder C:\users is kept: it holds your user folder' ] || fail "C:\\users: $out"
out=$(drop 'C:\Junc') && fail "the junction was deleted: $out"
[ -d "$C/Real" ] || fail "C:\\Real went"
out=$(drop 'C:\Clips') || fail "C:\\Clips was not deleted: $out"
[ ! -e "$C/Clips" ] || fail "C:\\Clips stays: $(ls "$C/Clips")"
[ -f "$C/Elsewhere/not the clips.txt" ] || fail "the folder the junction in the clips pointed to was emptied"
ok "the removal: C:\\Clips deleted, the junction in it as a link and C:\\Elsewhere whole; C:\\users and a junction kept, untouched"
wineserver -k 2>/dev/null || true
