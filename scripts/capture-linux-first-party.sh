#!/usr/bin/env bash
set -euo pipefail

fixture=${1:-fixtures/real-figma}
fixture=$(realpath "$fixture")
repo=$(git rev-parse --show-toplevel)
cargo_bin=${CARGO:-cargo}
capture_bin="$repo/target/debug/capture-real-group"
verify_bin="$repo/target/debug/figma-rust"
evidence_report=${EVIDENCE_REPORT:-"${TMPDIR:-/tmp}/figma-rust-linux-capture-evidence-$$.json"}

for command in base64 flock gnome-screenshot jq realpath sha256sum "$cargo_bin"; do
    command -v "$command" >/dev/null || {
        printf 'required command is unavailable: %s\n' "$command" >&2
        exit 2
    }
done

[[ ${XDG_SESSION_TYPE:-} == wayland ]] || {
    printf 'first-party capture requires XDG_SESSION_TYPE=wayland\n' >&2
    exit 2
}
[[ -n ${WAYLAND_DISPLAY:-} ]] || {
    printf 'first-party capture requires WAYLAND_DISPLAY\n' >&2
    exit 2
}
[[ -d $fixture ]] || {
    printf 'fixture directory does not exist: %s\n' "$fixture" >&2
    exit 2
}

lock_key=$(printf '%s' "$fixture" | sha256sum | cut -d' ' -f1)
lock_path="${TMPDIR:-/tmp}/figma-rust-capture-$lock_key.lock"
exec 9>"$lock_path"
flock -n 9 || {
    printf 'another capture owns fixture: %s\n' "$fixture" >&2
    exit 2
}

if pgrep -f '[/]capture-real-group --display' >/dev/null; then
    printf 'stale capture-real-group display process exists\n' >&2
    exit 2
fi

"$cargo_bin" build -p figma-generated-gpui-fixture --features capture-real-window --bin capture-real-group
"$cargo_bin" build -p figma-rust-cli

work=$(mktemp -d "${TMPDIR:-/tmp}/figma-rust-first-party-capture.XXXXXX")
active_pid=
cleanup() {
    if [[ -n ${active_pid:-} ]] && kill -0 "$active_pid" 2>/dev/null; then
        kill "$active_pid" 2>/dev/null || true
        wait "$active_pid" 2>/dev/null || true
    fi
    rm -rf "$work"
}
trap cleanup EXIT INT TERM

capture_backdrop() {
    local backdrop=$1
    local final=$2
    local ready="$work/$backdrop.ready.json"
    local stderr="$work/$backdrop.stderr"
    local screenshot="$work/$backdrop.png"

    "$capture_bin" --display "$backdrop" >"$ready" 2>"$stderr" &
    active_pid=$!

    for _ in {1..150}; do
        if [[ -s $ready ]]; then
            break
        fi
        kill -0 "$active_pid" 2>/dev/null || {
            cat "$stderr" >&2
            printf '%s display exited before readiness\n' "$backdrop" >&2
            exit 2
        }
        sleep 0.1
    done

    [[ $(wc -l <"$ready") -eq 1 ]] || {
        printf '%s display emitted an invalid readiness record\n' "$backdrop" >&2
        exit 2
    }
    jq -e \
        --argjson pid "$active_pid" \
        --arg backdrop "$backdrop" \
        '.schema_version == 1 and .event == "ready" and .pid == $pid and
         .backdrop == $backdrop and .logical_width == 100 and .logical_height == 60 and
         .window_title == "figma-rust-real-group-ready" and
         .wm_class == "figma-rust-real-group" and
         .gpui_revision == "5631830c564afa89b3aba679f45d9c3345f9460f" and
         .text_rendering == "grayscale"' "$ready" >/dev/null

    sleep 0.5
    gnome-screenshot -w -f "$screenshot"
    {
        printf 'data:image/png;base64,'
        base64 -w0 "$screenshot"
    } | "$capture_bin" --ingest-gnome-data-url "$final"

    kill "$active_pid"
    wait "$active_pid" || true
    active_pid=
    printf '%s\t%s\n' "$backdrop" "$(jq -r .pid "$ready")" >>"$work/pids.tsv"
}

black="$fixture/capture.black.png"
white="$fixture/capture.white.png"
actual="$fixture/actual.png"
manifest="$fixture/verify.image.json"
report="$fixture/capture.verify.json"
metadata="$fixture/capture.compositor.json"

capture_backdrop black "$black"
capture_backdrop white "$white"
"$capture_bin" --reconstruct "$black" "$white" "$actual"

report_tmp=$(mktemp "$fixture/.capture.verify.json.tmp-XXXXXX")
"$verify_bin" verify "$manifest" --json >"$report_tmp"
jq -e '.passed == true' "$report_tmp" >/dev/null
sync "$report_tmp"
mv -f "$report_tmp" "$report"

"$capture_bin" --finalize-provenance \
    "$metadata" "$black" "$white" "$actual" "$manifest" "$report" \
    gnome-shell-screenshot
"$verify_bin" verify "$manifest" --json >/dev/null

black_pid=$(awk '$1 == "black" { print $2 }' "$work/pids.tsv")
white_pid=$(awk '$1 == "white" { print $2 }' "$work/pids.tsv")
mkdir -p "$(dirname "$evidence_report")"
jq -n \
    --arg adapter gnome-shell-screenshot \
    --arg session "$XDG_SESSION_TYPE" \
    --arg wayland "$WAYLAND_DISPLAY" \
    --arg screenshot_version "$(gnome-screenshot --version 2>&1)" \
    --arg black_pid "$black_pid" \
    --arg white_pid "$white_pid" \
    --arg black_sha "$(sha256sum "$black" | cut -d' ' -f1)" \
    --arg white_sha "$(sha256sum "$white" | cut -d' ' -f1)" \
    --arg actual_sha "$(sha256sum "$actual" | cut -d' ' -f1)" \
    --arg report_sha "$(sha256sum "$report" | cut -d' ' -f1)" \
    '{schema_version: 1, adapter: $adapter, computer_use: false,
      session_type: $session, wayland_display: $wayland,
      screenshot_tool: $screenshot_version,
      captures: {black: {pid: ($black_pid | tonumber), sha256: $black_sha},
                 white: {pid: ($white_pid | tonumber), sha256: $white_sha}},
      actual_sha256: $actual_sha, verifier_report_sha256: $report_sha,
      dimensions: {width: 100, height: 60, scale: 1}, passed: true}' \
    >"$evidence_report"

if pgrep -f '[/]capture-real-group --display' >/dev/null; then
    printf 'capture process remained after cleanup\n' >&2
    exit 2
fi
if find "$fixture" -maxdepth 1 -name '.*.tmp-*' -print -quit | grep -q .; then
    printf 'temporary capture artifact remained in fixture\n' >&2
    exit 2
fi

printf 'first-party capture passed: %s\n' "$evidence_report"
