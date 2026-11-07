#!/usr/bin/env bash
# Rode UMA vez, como root:  sudo bash tools/wake/install.sh
# Instala o auxiliar que arma o alarme de acordar e libera só ele, sem senha, para o seu usuário.
set -euo pipefail
[[ $EUID -eq 0 ]] || { echo "rode com sudo" >&2; exit 1; }
user="${SUDO_USER:?rode com sudo, não direto como root}"
here="$(cd "$(dirname "$0")" && pwd)"
install -o root -g root -m 755 "$here/telacast-wake" /usr/local/sbin/telacast-wake
tmp="$(mktemp)"
echo "$user ALL=(root) NOPASSWD: /usr/local/sbin/telacast-wake" > "$tmp"
visudo -cf "$tmp" >/dev/null
install -o root -g root -m 440 "$tmp" /etc/sudoers.d/telacast-wake
rm -f "$tmp"
echo "pronto: $user pode rodar 'sudo -n telacast-wake <segundos>'"
