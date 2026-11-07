#!/usr/bin/env bash
# Um ciclo de espera: arma o alarme do RTC, suspende o PC e, ao acordar, segura o PC acordado.
#   cycle.sh [total_min=15] [antecedencia_min=2] [dry]
# Exemplo (15 min no total): alarme em 13 min; o PC acorda e fica 2 min acordado (rede volta);
# o comando termina aos 15 min e é aí que eu retomo o trabalho.
# Segurança: se o alarme não for armado e conferido, NÃO suspende.
set -euo pipefail
total="${1:-15}"; lead="${2:-2}"; mode="${3:-}"
dir="$(cd "$(dirname "$0")" && pwd)"
(( total > lead && total <= 60 && lead >= 1 )) || { echo "parâmetros inválidos" >&2; exit 2; }
secs=$(( (total - lead) * 60 ))

"$dir/awake.sh" stop >/dev/null
sudo -n /usr/local/sbin/telacast-wake "$secs" >/dev/null
alarm="$(cat /sys/class/rtc/rtc0/wakealarm)"
now="$(date +%s)"
if [[ -z "$alarm" ]] || (( alarm < now + secs - 5 || alarm > now + secs + 5 )); then
  echo "ERRO: alarme não conferiu (alarme='$alarm', esperado ~$((now+secs))). Não vou suspender." >&2
  exit 1
fi
echo "alarme armado para $(date -d "@$alarm" +%H:%M:%S) (daqui a $((secs/60)) min); suspendendo às $(date +%H:%M:%S)"
[[ "$mode" == "dry" ]] && { sudo -n /usr/local/sbin/telacast-wake off >/dev/null; echo "(dry: não suspendi e cancelei o alarme)"; exit 0; }
sync; sleep 2
systemctl suspend
echo "acordei às $(date +%H:%M:%S)"
"$dir/awake.sh" start >/dev/null
sleep "$(( lead * 60 ))"
echo "ciclo completo às $(date +%H:%M:%S)"
