#!/usr/bin/env bash
# Mantém o PC acordado (sem suspender por inatividade) enquanto eu trabalho.
#   awake.sh start | stop | status
pidfile="${XDG_RUNTIME_DIR:-/tmp}/telacast-awake.pid"
case "${1:-status}" in
  start)
    "$0" stop
    setsid systemd-inhibit --what=idle --who=telacast --why="trabalho noturno" \
      kde-inhibit --power --screenSaver sleep infinity >/dev/null 2>&1 &
    echo $! > "$pidfile"; echo "acordado (pid $(cat "$pidfile"))" ;;
  stop)
    if [[ -f "$pidfile" ]]; then
      pid="$(cat "$pidfile")"; pkill -P "$pid" 2>/dev/null || true; kill "$pid" 2>/dev/null || true
      pkill -f "kde-inhibit --power --screenSaver sleep infinity" 2>/dev/null || true
      rm -f "$pidfile"
    fi; echo "liberado" ;;
  status) [[ -f "$pidfile" ]] && kill -0 "$(cat "$pidfile")" 2>/dev/null && echo "segurando acordado" || echo "livre" ;;
esac
