#!/bin/bash
# Regrava os PNGs das telas (Paparazzi, na JVM, sem rede) e copia para docs/telas/<nome>.png.
# Só rode quando o visual mudar de propósito: o `verifyPaparazziDebug` falha se algum PNG mudar sem regravar.
set -e
cd "$(dirname "$0")/.."
scripts/gradle.sh recordPaparazziDebug
mkdir -p docs/telas
rm -f docs/telas/*.png
for f in app/src/test/snapshots/images/app.telacast_TelasSnapshotTest_*.png; do
  nome="$(basename "$f" .png)"
  nome="${nome#app.telacast_TelasSnapshotTest_}"   # o Paparazzi repete o nome: <teste>_<nome>
  metade=$(( (${#nome} - 1) / 2 ))
  cp "$f" "docs/telas/${nome:0:$metade}.png"
done
cp app/src/test/snapshots/images/app.telacast_PaletaSnapshotTest_paleta.png docs/paleta.png
echo "ok: $(ls docs/telas | wc -l) telas em docs/telas/"
