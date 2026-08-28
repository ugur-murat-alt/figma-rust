#!/usr/bin/env bash
set -euo pipefail

baseline=5631830c564afa89b3aba679f45d9c3345f9460f
candidate=${1:-$baseline}
output=${2:-"${TMPDIR:-/tmp}/figma-rust-gpui-qualification.json"}
mode=${3:-pass}
repo=$(git rev-parse --show-toplevel)
cargo_bin=${CARGO:-cargo}

[[ $candidate =~ ^[0-9a-f]{40}$ ]] || {
    printf 'candidate revision must be 40 lowercase hexadecimal characters\n' >&2
    exit 2
}
[[ $mode == pass || $mode == expect-api-failure ]] || {
    printf 'mode must be pass or expect-api-failure\n' >&2
    exit 2
}
for command in git jq perl sha256sum tar "$cargo_bin"; do
    command -v "$command" >/dev/null || {
        printf 'required command is unavailable: %s\n' "$command" >&2
        exit 2
    }
done

source_commit=$(git -C "$repo" rev-parse HEAD)
work=$(mktemp -d "${TMPDIR:-/tmp}/figma-rust-gpui-qualification.XXXXXX")
snapshot="$work/repository"
logs="$output.logs"
cleanup() {
    rm -rf "$work"
}
trap cleanup EXIT INT TERM
mkdir -p "$snapshot" "$logs" "$(dirname "$output")"
git -C "$repo" archive "$source_commit" | tar -xf - -C "$snapshot"

mapfile -t pin_bindings < <(
    grep -Ilr --exclude-dir=.git "$baseline" "$snapshot" \
        | sed "s#^$snapshot/##" \
        | LC_ALL=C sort
)
if ((${#pin_bindings[@]} == 0)); then
    printf 'baseline GPUI revision is not bound in the qualified source\n' >&2
    exit 2
fi

if [[ $candidate != "$baseline" ]]; then
    perl -pi -e "s/$baseline/$candidate/g" \
        "$snapshot/Cargo.toml" \
        "$snapshot/fixtures/generated-gpui/Cargo.toml"
fi
if [[ $mode == expect-api-failure ]]; then
    cat >>"$snapshot/fixtures/generated-gpui/src/lib.rs" <<'RUST'

#[doc(hidden)]
pub fn deliberately_incompatible_gpui_api_probe() {
    let _ = gpui::figma_rust_missing_candidate_api();
}
RUST
fi

export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-"$repo/target/gpui-qualification"}
(
    cd "$snapshot"
    "$cargo_bin" clean -p figma-generated-gpui-fixture
) >/dev/null 2>&1
stage_names=(api_compile runtime_helpers geometry pixels)
declare -A stage_status stage_exit

run_stage() {
    local name=$1
    shift
    set +e
    (cd "$snapshot" && "$@") >"$logs/$name.stdout" 2>"$logs/$name.stderr"
    local exit_code=$?
    set -e
    stage_exit[$name]=$exit_code
    if ((exit_code == 0)); then
        stage_status[$name]=PASS
    else
        stage_status[$name]=FAIL
    fi
}

run_stage api_compile "$cargo_bin" check -p figma-generated-gpui-fixture \
    --features capture-real-window --all-targets

if [[ $mode == expect-api-failure ]]; then
    if ((stage_exit[api_compile] == 0)); then
        passed=false
        classification=UNEXPECTED_API_COMPATIBILITY
    else
        passed=true
        classification=GPUI_API_COMPILE_FAILED
        stage_status[api_compile]=EXPECTED_FAIL
    fi
    for stage in runtime_helpers geometry pixels; do
        stage_status[$stage]=SKIPPED
        stage_exit[$stage]=-1
    done
else
    if ((stage_exit[api_compile] == 0)); then
        run_stage runtime_helpers "$cargo_bin" test -p figma-generated-gpui-fixture \
            --features capture-real-window
    else
        stage_status[runtime_helpers]=BLOCKED
        stage_exit[runtime_helpers]=-1
    fi

    if [[ ${stage_status[runtime_helpers]} == PASS ]]; then
        run_stage geometry "$cargo_bin" run -p figma-rust-cli -- verify \
            fixtures/real-figma/verify.geometry.json --json
    else
        stage_status[geometry]=BLOCKED
        stage_exit[geometry]=-1
    fi

    if [[ ${stage_status[geometry]} == PASS ]]; then
        run_stage pixels "$cargo_bin" run -p figma-rust-cli -- verify \
            fixtures/real-figma/verify.image.json --json
    else
        stage_status[pixels]=BLOCKED
        stage_exit[pixels]=-1
    fi

    passed=true
    classification=QUALIFIED
    for stage in "${stage_names[@]}"; do
        if [[ ${stage_status[$stage]} != PASS ]]; then
            passed=false
            classification=${stage^^}_FAILED
            break
        fi
    done
fi

case $(uname -s) in
    Darwin)
        capture_capability=HEADLESS_RENDERER_AVAILABLE
        ;;
    Linux)
        capture_capability=COMPOSITOR_REQUIRED
        ;;
    MINGW*|MSYS*|CYGWIN*)
        capture_capability=HEADLESS_RENDERER_UNAVAILABLE
        ;;
    *)
        capture_capability=UNCLASSIFIED_PLATFORM
        ;;
esac

bindings_json=$(printf '%s\n' "${pin_bindings[@]}" | jq -R . | jq -s .)
if [[ $candidate == "$baseline" ]]; then
    migrations_json='[]'
else
    migrations_json=$bindings_json
fi
stages_json=$(jq -n \
    --arg api_status "${stage_status[api_compile]}" \
    --argjson api_exit "${stage_exit[api_compile]}" \
    --arg runtime_status "${stage_status[runtime_helpers]}" \
    --argjson runtime_exit "${stage_exit[runtime_helpers]}" \
    --arg geometry_status "${stage_status[geometry]}" \
    --argjson geometry_exit "${stage_exit[geometry]}" \
    --arg pixels_status "${stage_status[pixels]}" \
    --argjson pixels_exit "${stage_exit[pixels]}" \
    '{api_compile:{status:$api_status,exit_code:$api_exit},
      runtime_helpers:{status:$runtime_status,exit_code:$runtime_exit},
      geometry:{status:$geometry_status,exit_code:$geometry_exit},
      pixels:{status:$pixels_status,exit_code:$pixels_exit}}')

temporary=$(mktemp "$(dirname "$output")/.gpui-qualification.tmp-XXXXXX")
jq -n -S \
    --arg baseline_revision "$baseline" \
    --arg candidate_revision "$candidate" \
    --arg source_commit "$source_commit" \
    --arg mode "$mode" \
    --arg classification "$classification" \
    --arg capture_capability "$capture_capability" \
    --argjson passed "$passed" \
    --argjson pin_bindings "$bindings_json" \
    --argjson required_migrations "$migrations_json" \
    --argjson stages "$stages_json" \
    '{schema_version:1, baseline_revision:$baseline_revision,
      candidate_revision:$candidate_revision, source_commit:$source_commit,
      mode:$mode, passed:$passed, classification:$classification,
      capture_capability:$capture_capability, pin_bindings:$pin_bindings,
      required_migrations:$required_migrations, stages:$stages}' \
    >"$temporary"
sync "$temporary"
mv -f "$temporary" "$output"

printf 'GPUI qualification %s: %s (%s)\n' "$classification" "$output" \
    "$(sha256sum "$output" | cut -d' ' -f1)"
if [[ $passed != true ]]; then
    exit 1
fi
