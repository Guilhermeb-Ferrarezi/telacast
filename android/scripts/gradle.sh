#!/bin/bash
# Roda o Gradle 8.9 da máquina com a receita de pouca RAM (docs/MOBILE-PLAN.md, seção 4, item 3).
# Uso (dentro de android/ ou fora): scripts/gradle.sh assembleDebug | recordPaparazziDebug | verifyPaparazziDebug
# Recusa rodar com menos de 3 GB disponíveis. ANDROID_HOME e Gradle podem ser trocados por variável.
set -e
cd "$(dirname "$0")/.."
disp=$(free -m | awk '/^Mem:/ {print $7}')
if [ "$disp" -lt 3000 ]; then
  echo "só ${disp} MB disponíveis (precisa de 3000): não compilo agora; tente de novo em 20-30 min" >&2
  exit 75
fi
exec systemd-run --user --scope -q -p MemoryMax=2500M -p MemorySwapMax=0 nice -n 19 \
  env ANDROID_HOME="${ANDROID_HOME:-$HOME/Android/Sdk}" GRADLE_OPTS=-Xmx512m \
  "${GRADLE_BIN:-$HOME/Android/gradle-8.9/bin/gradle}" --no-daemon --max-workers=2 "$@"
