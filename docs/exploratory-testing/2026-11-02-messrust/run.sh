#!/bin/sh
set -eu
cd /workspace
base=docs/exploratory-testing/2026-11-02-messrust
work="$base/scratch"
rm -rf "$work"
mkdir -p "$work"
trap 'rm -rf "$work"' EXIT INT TERM
bin=./target/release/messrust
run() {
    printf '\n$ %s\n' "$*"
    set +e
    "$@"
    status=$?
    set -e
    printf 'exit=%s\n' "$status"
}
{
    "$bin" --version
    run "$bin" "$base/fixtures/service.rs" text rust --only UnusedLocalVariable
    run "$bin" "$base/fixtures/service.rs" text rust --only UnusedLocalVariable --ignore-tests --strict
    run "$bin" "$base/fixtures/service.rs" json rust --only UnusedLocalVariable --ignore-tests --reportfile "$work/report.json" --ignore-violations-on-exit
    printf 'report file contents:\n'
    python3 -m json.tool "$work/report.json"
    run "$bin" "$base/fixtures/params.rs" text "$base/policy.xml"
    run "$bin" "$base/fixtures/params.rs" text "$base/policy.xml,codesize" --only ExcessiveParameterList
    run "$bin" "$base/fixtures/params.rs" text "codesize,$base/policy.xml" --only ExcessiveParameterList
    for iteration in 1 2; do
        printf '\nreplay=%s\n' "$iteration"
        rm -f "$work/errors.json" "$work/errors.sarif"
        run "$bin" "$base/fixtures/broken.rs,$base/fixtures/service.rs" json rust --only UnusedLocalVariable --reportfile "$work/errors.json"
        printf 'json report contents:\n'
        python3 -m json.tool "$work/errors.json"
        run "$bin" "$base/fixtures/broken.rs,$base/fixtures/service.rs" sarif rust --only UnusedLocalVariable --reportfile "$work/errors.sarif"
        printf 'sarif report contents:\n'
        python3 -m json.tool "$work/errors.sarif"
    done
} > "$base/evidence/journeys.txt" 2>&1
