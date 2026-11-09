#!/usr/bin/env bash
args=()
while (($#)); do
  case $1 in
    --target=*) shift ;;
    -target)
      shift
      (($#)) && shift
      ;;
    *)
      args+=("$1")
      shift
      ;;
  esac
done
exec zig cc -target x86_64-linux-musl "${args[@]}"
