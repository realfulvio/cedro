#!/usr/bin/env bash
# Installa Cedro per l'utente corrente: nessun permesso di amministratore.
#   ./install.sh              copia Cedro in ~/.local/opt/cedro e lo collega in ~/.local/bin
#   ./install.sh --no-start   come sopra, senza avviare Cedro (lo usa l'aggiornamento da dentro)
#   ./install.sh --uninstall  toglie Cedro e i menu che ha scritto (le impostazioni restano)
# Serve GTK 4 (su Arch gtk4, su Debian/Ubuntu libgtk-4-1, su Fedora gtk4).
set -euo pipefail
here=$(cd "$(dirname "$0")" && pwd)
dest=$HOME/.local/opt/cedro
bindir=$HOME/.local/bin
data=${XDG_DATA_HOME:-$HOME/.local/share}
config=${XDG_CONFIG_HOME:-$HOME/.config}

if [ "${1:-}" = "--uninstall" ]; then
  pkill -x cedro 2>/dev/null || true
  rm -f "$bindir/cedro"
  rm -rf "$dest"
  rm -f "$data/applications/dev.cedro.Cedro.desktop" "$data/icons/hicolor/256x256/apps/dev.cedro.Cedro.png" \
        "$data/kio/servicemenus/cedro.desktop" "$data/nautilus-python/extensions/cedro.py" "$config/autostart/dev.cedro.Cedro.desktop"
  rm -rf "$data/nautilus/scripts/Cedro"
  echo "Cedro tolto. Le impostazioni restano in $config/cedro (puoi cancellarle a mano)."
  exit 0
fi

have_gtk() {
  for d in /usr/lib /usr/lib64 /usr/lib/x86_64-linux-gnu /lib/x86_64-linux-gnu /usr/local/lib; do
    ls "$d"/libgtk-4.so.1 >/dev/null 2>&1 && return 0
  done
  { /sbin/ldconfig -p || ldconfig -p; } 2>/dev/null | grep -q 'libgtk-4\.so'
}
# L'aggiornamento da dentro (--no-start) parte da un Cedro già in esecuzione: GTK c'è.
if [ "${1:-}" != "--no-start" ] && ! have_gtk; then
  echo "Manca GTK 4: installa il pacchetto gtk4 (Arch), libgtk-4-1 (Debian, Ubuntu) o gtk4 (Fedora)." >&2
  exit 1
fi
mkdir -p "$dest" "$bindir"
install -m 755 "$here/cedro" "$dest/cedro"
# Il file delle licenze sta accanto al programma: la scheda «Informazioni» lo cerca lì.
[ -f "$here/Licenze di terze parti.txt" ] && install -m 644 "$here/Licenze di terze parti.txt" "$dest/"
[ -f "$here/LICENSE" ] && install -m 644 "$here/LICENSE" "$dest/"
ln -sf "$dest/cedro" "$bindir/cedro"
echo "Cedro installato in $dest."
case ":$PATH:" in
  *":$bindir:"*) ;;
  *) echo "Aggiungi $bindir al PATH per avviarlo con «cedro»." ;;
esac
# La prima volta Cedro scrive da solo i menu dei file (Dolphin, Nautilus) e l'icona.
if [ "${1:-}" != "--no-start" ]; then
  nohup "$dest/cedro" --background >/dev/null 2>&1 &
  echo "Avviato: l'icona di Cedro è vicino all'orologio. Su GNOME serve l'estensione AppIndicator."
fi
