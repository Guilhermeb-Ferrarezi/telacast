#!/bin/bash
# Regrava o PNG de referência da paleta (Paparazzi, na JVM) e copia para docs/paleta.png.
set -e
cd "$(dirname "$0")/.."
scripts/gradle.sh recordPaparazziDebug
cp app/src/test/snapshots/images/app.telacast_PaletaSnapshotTest_paleta.png docs/paleta.png
echo "ok: docs/paleta.png"
