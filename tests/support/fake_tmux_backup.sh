#!/bin/sh
set -eu

if [ -n "${REMUX_FAKE_LOG:-}" ]; then
  printf '%s\n' "$*" >> "$REMUX_FAKE_LOG"
fi

fail_socket=0
if [ "${1:-}" = "-L" ]; then
  if [ -n "${REMUX_FAKE_FAIL_SOCKET:-}" ] && [ "${2:-}" = "${REMUX_FAKE_FAIL_SOCKET}" ]; then
    fail_socket=1
  fi
  shift 2
fi

case "${1:-}" in
  list-sessions)
    if [ "${REMUX_FAKE_NO_SERVER:-0}" = "1" ]; then
      printf 'no server running on fake\n' >&2
      exit 1
    fi
    printf 'work:=:(120,40):=:0\n'
    ;;
  list-windows)
    if [ "$fail_socket" = "1" ]; then
      exit 1
    fi
    target=''
    shift
    while [ "$#" -gt 0 ]; do
      case "$1" in
        -t*)
          target="${1#-t}"
          ;;
      esac
      shift
    done
    case "$target" in
      work)
        printf '1:=:editor:=:1:=:1900,120x40,0,0,0\n'
        ;;
      *)
        exit 1
        ;;
    esac
    ;;
  list-panes)
    target=''
    shift
    while [ "$#" -gt 0 ]; do
      case "$1" in
        -t*)
          target="${1#-t}"
          ;;
      esac
      shift
    done
    case "$target" in
      work:1)
        pid="${REMUX_FAKE_PANE_PID:-$$}"
        printf '0:=:(120,20):=:/tmp/work:=:1:=:sh:=:%s\n1:=:(120,20):=:/tmp/logs:=:0:=:sh:=:%s\n' "$pid" "$pid"
        ;;
      *)
        exit 1
        ;;
    esac
    ;;
  capture-pane)
    target=''
    flag=''
    shift
    while [ "$#" -gt 0 ]; do
      case "$1" in
        -ep|-p)
          flag="$1"
          ;;
        -t*)
          target="${1#-t}"
          ;;
      esac
      shift
    done
    case "$target" in
      work:1.0)
        if [ "$flag" = "-ep" ]; then
          printf 'pane0 with escape \033[31mred\033[0m\n'
        else
          printf 'pane0 plain\n'
        fi
        ;;
      work:1.1)
        if [ "$flag" = "-ep" ]; then
          printf 'pane1 with escape \033[32mgreen\033[0m\n'
        else
          printf 'pane1 plain\n'
        fi
        ;;
      *)
        exit 1
        ;;
    esac
    ;;
  *)
    exit 1
    ;;
esac
