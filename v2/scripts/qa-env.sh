#!/bin/sh
# Compatibility entry point: execute a command in the same isolated QA environment.
exec "$(dirname -- "$0")/qa-run.sh" "$@"
