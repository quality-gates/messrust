#!/bin/sh
# Replay driver: runs the built messrust binary in a capped container.
# Usage: run.sh <messrust args...>   (paths relative to this directory)
cd "$(dirname "$0")"
docker run --rm --cpus=2 --memory=2g -v "$PWD":/et -w /et -v messrust-et-target-20260926:/target:ro messrust-dev \
  sh -c '/target/debug/messrust "$@"; echo "[exit=$?]"' messrust "$@"
