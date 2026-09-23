#!/bin/sh
# Replay driver: runs the branch binary in a capped container.
# Usage: run.sh <messrust args...>   (paths relative to the evidence folder root)
# Needs the docker volume messrust-et-target with a built /target/debug/messrust.
cd "$(dirname "$0")"
docker run --rm --cpus=2 --memory=2g -v "$PWD":/et -w /et -v messrust-et-target:/target:ro messrust-dev \
  sh -c '/target/debug/messrust "$@"; echo "[exit=$?]"' messrust "$@"
