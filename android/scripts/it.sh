#!/bin/bash
# Testes de INTEGRAÇÃO: sobe o servidor REAL (target/release/telacast-server) num banco DESCARTÁVEL e numa porta
# livre, cria um convite pela CLI do servidor, roda os testes Kotlin `*IntegracaoTest` contra ele e derruba tudo no fim
# (mata o servidor, apaga o banco e a pasta de anexos), mesmo em caso de erro ou Ctrl+C.
#
#   android/scripts/it.sh            # roda tudo (leva ~1 min: a paginação respeita o limite de 12 envios por 10 s)
#   android/scripts/it.sh --tests '*IntegracaoTest.digitando*'   # argumentos extras vão para o Gradle
#
# NUNCA toca no banco `telacast` de uso nem na porta 8080. Precisa do Postgres de DEV em 127.0.0.1:5432
# (credenciais de dev de server/tests/e2e.rs; troque com ADMIN_URL=postgres://usuario:senha@host:porta/postgres).
# Variáveis opcionais: SERVER_BIN (outro binário do servidor).
set -uo pipefail
RAIZ="$(cd "$(dirname "$0")/../.." && pwd)"
ANDROID="$RAIZ/android"
BIN="${SERVER_BIN:-$RAIZ/target/release/telacast-server}"
ADMIN_URL="${ADMIN_URL:-postgres://telacast:telacast_dev@127.0.0.1:5432/postgres}"
export ADMIN_URL

if [ ! -x "$BIN" ]; then
  echo "binário do servidor não existe: $BIN" >&2
  echo "compile antes (confira free -m, precisa de ~3 GB): cargo build --release -p telacast-server -j2" >&2
  exit 2
fi

NANOS="$(date +%s%N)"
DB="telacast_it_${NANOS}"
TMP="$(mktemp -d /tmp/telacast_it_XXXXXX)"
SERVER_PID=""

limpar() {
  if [ -n "$SERVER_PID" ]; then kill "$SERVER_PID" 2>/dev/null; wait "$SERVER_PID" 2>/dev/null; fi
  python3 "$ANDROID/scripts/pg.py" drop "$DB" >/dev/null 2>&1 && echo "banco $DB apagado"
  rm -rf "$TMP"
}
trap limpar EXIT
trap 'exit 130' INT TERM

python3 "$ANDROID/scripts/pg.py" create "$DB" || { echo "não consegui criar o banco de teste (o Postgres de dev está no ar?)" >&2; exit 3; }

# usuário e senha do ADMIN_URL valem para o banco novo (mesmo servidor Postgres)
DB_URL="$(python3 - "$ADMIN_URL" "$DB" <<'PY'
import sys
from urllib.parse import urlparse
u = urlparse(sys.argv[1])
print(f"postgres://{u.username}:{u.password}@{u.hostname}:{u.port or 5432}/{sys.argv[2]}")
PY
)"
PORTA="$(python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1])')"
if [ "$PORTA" = "8080" ]; then echo "porta 8080 é do servidor de uso; abortando" >&2; exit 4; fi

# atualização do app: chave de teste, um "APK" falso e o manifesto assinado para o teste de integração conferir
export TELACAST_UPDATES_DIR="$TMP/updates"; mkdir -p "$TELACAST_UPDATES_DIR"
CHAVE_PUBLICA="$("$BIN" update-keygen --out "$TMP/update.key" 2>&1 | tail -1 | tr -d '[:space:]')"
head -c 262144 /dev/urandom > "$TMP/falso.apk"
TELACAST_UPDATE_KEY_FILE="$TMP/update.key" "$BIN" publish-update --file "$TMP/falso.apk" --version 9.9.9 --platform android-arm64 --notes "Teste de integração: nada mudou." >/dev/null 2>&1 || echo "aviso: não consegui publicar a atualização de teste" >&2
export TELACAST_IT_UPDATE_PUBKEY="$CHAVE_PUBLICA" TELACAST_IT_UPDATE_SHA="$(sha256sum "$TMP/falso.apk" | cut -d' ' -f1)"

DATABASE_URL="$DB_URL" TELACAST_ADDR="127.0.0.1:$PORTA" RUST_LOG=warn TELACAST_FILES_DIR="$TMP/files" \
  "$BIN" serve >"$TMP/servidor.log" 2>&1 &
SERVER_PID=$!

for _ in $(seq 1 100); do
  if (exec 3<>"/dev/tcp/127.0.0.1/$PORTA") 2>/dev/null; then break; fi
  kill -0 "$SERVER_PID" 2>/dev/null || { echo "o servidor morreu ao subir:" >&2; cat "$TMP/servidor.log" >&2; exit 5; }
  sleep 0.2
done
(exec 3<>"/dev/tcp/127.0.0.1/$PORTA") 2>/dev/null || { echo "o servidor não abriu a porta $PORTA" >&2; cat "$TMP/servidor.log" >&2; exit 5; }

CONVITE="$(DATABASE_URL="$DB_URL" RUST_LOG=warn "$BIN" invite --uses 30 --days 1 --label it 2>&1 | sed -n 's/^Código de convite: //p' | head -1 | tr -d '[:space:]')"
[ -n "$CONVITE" ] || { echo "não consegui criar o convite" >&2; exit 6; }
echo "servidor de teste em 127.0.0.1:$PORTA (banco $DB), convite criado"

export TELACAST_IT_URL="127.0.0.1:$PORTA"
export TELACAST_IT_INVITE="$CONVITE"
cd "$ANDROID" && scripts/gradle.sh testDebugUnitTest -PintegrationTests "$@"
STATUS=$?
if [ $STATUS -ne 0 ] && [ -s "$TMP/servidor.log" ]; then echo "--- log do servidor ---"; tail -n 30 "$TMP/servidor.log"; fi
exit $STATUS
