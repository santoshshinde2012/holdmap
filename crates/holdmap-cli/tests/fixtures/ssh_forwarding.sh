#!/bin/sh
port=
while [ "$#" -gt 0 ]; do
  case "$1" in
    -o) shift 2 ;;
    -p) port=$2; shift 2 ;;
    -t) shift ;;
    --) shift; break ;;
    *) exit 80 ;;
  esac
done
[ "$port" = 2222 ] && [ "$1" = 'user@fixture.invalid' ] || exit 81
shift
exec /bin/sh -c "$*"
