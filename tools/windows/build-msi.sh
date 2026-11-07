#!/usr/bin/env bash
# Gera dist/Telacast-<versão>-x64.msi a partir do Telacast.exe cruzado (zig) e do wixl (pacote msitools/wixl).
# Uso: tools/windows/build-msi.sh [caminho-do-wixl]
set -euo pipefail
cd "$(dirname "$0")/../.."
WIXL="${1:-${WIXL:-wixl}}"
VERSION="$(grep -m1 '^version' app/Cargo.toml | sed 's/.*"\(.*\)".*/\1/')"
echo "Versão $VERSION"
cargo zigbuild --release --target x86_64-pc-windows-gnu -p telacast -j2
OUT="dist/msi-build"
rm -rf "$OUT" && mkdir -p "$OUT" dist
cp target/x86_64-pc-windows-gnu/release/telacast.exe "$OUT/Telacast.exe"
cp tools/windows/telacast.ico "$OUT/"
sed "s/@VERSION@/$VERSION/" tools/windows/telacast.wxs > "$OUT/telacast.wxs"
( cd "$OUT" && "$WIXL" -v -a x64 -o "../Telacast-$VERSION-x64.msi" telacast.wxs )
ls -la "dist/Telacast-$VERSION-x64.msi"
sha256sum "dist/Telacast-$VERSION-x64.msi"
