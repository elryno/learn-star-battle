#!/usr/bin/env bash
# Install a built release and its GNOME launcher. PREFIX can stage a local package.
set -euo pipefail
cd "$(dirname "$0")/.."
prefix="${PREFIX:-$HOME/.local}"
case "$prefix" in
    /*) ;;
    *) printf 'PREFIX must be an absolute path.\n' >&2; exit 2 ;;
esac
test -x target/release/star-battle-solver
install -Dm755 target/release/star-battle-solver "$prefix/bin/star-battle-solver"
install -Dm644 data/io.github.elryno.LearnStarBattle.svg "$prefix/share/icons/hicolor/scalable/apps/io.github.elryno.LearnStarBattle.svg"
python3 - "$prefix" <<'PY'
from pathlib import Path
import sys
prefix = Path(sys.argv[1])
executable = str(prefix / 'bin/star-battle-solver')
if '\n' in executable or '\r' in executable:
    raise SystemExit('PREFIX must not contain line breaks')
# Exec quoting is parsed twice: desktop string unescaping, then argument parsing.
quoted = executable.replace('\\', '\\\\\\\\').replace('"', '\\\\"').replace('`', '\\\\`').replace('$', '\\\\$').replace('%', '%%')
text = Path('data/io.github.elryno.LearnStarBattle.desktop.in').read_text().replace('@EXEC@', f'"{quoted}"')
destination = prefix / 'share/applications/io.github.elryno.LearnStarBattle.desktop'
destination.parent.mkdir(parents=True, exist_ok=True)
destination.write_text(text)
PY
desktop-file-validate "$prefix/share/applications/io.github.elryno.LearnStarBattle.desktop"
# Retire the previous desktop identity only after the replacement validates.
rm -f -- "$prefix/share/applications/io.github.starbattle.Solver.desktop" \
    "$prefix/share/icons/hicolor/scalable/apps/io.github.starbattle.Solver.svg"
if command -v update-desktop-database >/dev/null; then
    update-desktop-database "$prefix/share/applications"
fi
if command -v gtk-update-icon-cache >/dev/null && [[ -f "$prefix/share/icons/hicolor/index.theme" ]]; then
    gtk-update-icon-cache -q -t "$prefix/share/icons/hicolor"
fi
printf 'Installed Learn Star Battle in %s\n' "$prefix"
