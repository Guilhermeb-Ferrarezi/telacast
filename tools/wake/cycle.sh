#!/usr/bin/env bash
# Um ciclo de espera: arma o alarme do RTC, suspende o PC e, ao acordar, segura o PC acordado
# até completar o tempo total.
#   cycle.sh [total_min=15] [antecedencia_min=2] [dry]
# Com 15 e 2: o alarme toca aos 13 min, o PC fica acordado por 2 min (a rede volta) e o comando
# termina aos 15 min; é aí que eu retomo. Registro de cada ciclo em ~/.local/state/telacast/cycle.log.
# Segurança: se o alarme não for armado e conferido, NÃO suspende.
set -euo pipefail
total="${1:-15}"; lead="${2:-2}"; mode="${3:-}"
dir="$(cd "$(dirname "$0")" && pwd)"
log="$HOME/.local/state/telacast/cycle.log"; mkdir -p "$(dirname "$log")"
say() { echo "$*"; echo "$(date '+%F %T') $*" >> "$log"; }
(( total > lead && total <= 60 && lead >= 1 )) || { echo "parâmetros inválidos" >&2; exit 2; }
secs=$(( (total - lead) * 60 ))
start="$(date +%s)"; target=$(( start + total * 60 ))

"$dir/awake.sh" stop >/dev/null
sudo -n /usr/local/sbin/telacast-wake "$secs" >/dev/null
alarm="$(cat /sys/class/rtc/rtc0/wakealarm)"; now="$(date +%s)"
if [[ -z "$alarm" ]] || (( alarm < now + secs - 5 || alarm > now + secs + 5 )); then
  say "ERRO: alarme não conferiu (alarme='$alarm', esperado ~$((now+secs))); não vou suspender"; exit 1
fi
say "alarme armado para $(date -d "@$alarm" +%H:%M:%S) (em $((secs/60)) min); pedindo suspensão"
if [[ "$mode" == "dry" ]]; then
  sudo -n /usr/local/sbin/telacast-wake off >/dev/null; say "(dry: não suspendi e cancelei o alarme)"; exit 0
fi

sync; sleep 2
systemctl suspend
# `systemctl suspend` volta antes de o PC dormir: espero o salto do relógio (tempo de parede x tempo acordado).
gap="$(python3 - <<'PY'
import time
t0, m0 = time.time(), time.monotonic()
while True:
    time.sleep(1)
    gap = (time.time() - t0) - (time.monotonic() - m0)
    if gap > 3: print(int(gap)); break
    if time.monotonic() - m0 > 90: print(-1); break
PY
)"
if (( gap < 0 )); then
  say "ERRO: o PC não suspendeu em 90 s; cancelando o alarme"
  sudo -n /usr/local/sbin/telacast-wake off >/dev/null; exit 1
fi
if [[ -z "$(cat /sys/class/rtc/rtc0/wakealarm)" ]]; then cause="alarme do RTC"; else
  cause="OUTRA CAUSA (o alarme ainda estava pendente)"; sudo -n /usr/local/sbin/telacast-wake off >/dev/null; fi
say "acordei às $(date +%H:%M:%S) depois de ${gap}s dormindo; causa: $cause"

"$dir/awake.sh" start >/dev/null
while (( $(date +%s) < target )); do sleep 5; done
say "ciclo completo às $(date +%H:%M:%S) (planejado: $(date -d "@$target" +%H:%M:%S))"
