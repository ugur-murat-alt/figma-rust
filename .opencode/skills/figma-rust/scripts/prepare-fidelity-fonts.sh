#!/usr/bin/env bash
set -euo pipefail
umask 077

for command in curl unzip sha256sum fc-match flock stat sync mktemp; do
	if ! command -v "$command" >/dev/null 2>&1; then
		printf 'required command is missing: %s\n' "$command" >&2
		exit 1
	fi
done

root=${FIGMA_RUST_FIDELITY_FONT_ROOT:-"$HOME/.local/share/figma-rust/fonts/foundations-419-2"}
font_dir="$root/fonts"
cache_dir="$root/cache"
config="$root/fonts.conf"

if [[ "$root" != /* || "$root" == / || "$root" == *$'\n'* || "$root" == *$'\r'* || "$root" == *'<'* || "$root" == *'>'* || "$root" == *'&'* ]]; then
	printf 'font root must be a safe absolute directory: %s\n' "$root" >&2
	exit 1
fi
for directory in "$root" "$font_dir" "$cache_dir"; do
	if [[ -L "$directory" ]]; then
		printf 'font directory must not be a symlink: %s\n' "$directory" >&2
		exit 1
	fi
done
mkdir -p "$font_dir" "$cache_dir"
chmod 700 "$root" "$font_dir" "$cache_dir"

exec 9<"$root"
flock -x 9

inter_zip=''
jetbrains_zip=''
font_temp=''
config_temp=''
cleanup() {
	[[ -z "$inter_zip" ]] || rm -f -- "$inter_zip"
	[[ -z "$jetbrains_zip" ]] || rm -f -- "$jetbrains_zip"
	[[ -z "$font_temp" ]] || rm -f -- "$font_temp"
	[[ -z "$config_temp" ]] || rm -f -- "$config_temp"
}
trap cleanup EXIT

sha_matches() {
	local path=$1
	local expected=$2
	[[ -f "$path" && ! -L "$path" ]] || return 1
	[[ "$(stat -c '%h' "$path")" == 1 ]] || return 1
	[[ "$(sha256sum "$path" | cut -d' ' -f1)" == "$expected" ]]
}

download_archive() {
	local variable_name=$1
	local label=$2
	local url=$3
	local expected=$4
	local path
	path=$(mktemp "$root/.${label}.XXXXXX.zip")
	printf -v "$variable_name" '%s' "$path"
	curl --proto '=https' --tlsv1.2 --fail --location --silent --show-error \
		"$url" --output "$path"
	if ! sha_matches "$path" "$expected"; then
		printf '%s archive SHA-256 mismatch\n' "$label" >&2
		exit 1
	fi
}

publish_font() {
	local archive=$1
	local entry=$2
	local file_name=$3
	local expected=$4
	local final="$font_dir/$file_name"

	if sha_matches "$final" "$expected"; then
		return
	fi
	if [[ -L "$final" ]]; then
		printf 'font output must not be a symlink: %s\n' "$final" >&2
		exit 1
	fi

	font_temp=$(mktemp "$font_dir/.${file_name}.XXXXXX")
	unzip -p "$archive" "$entry" >"$font_temp"
	if ! sha_matches "$font_temp" "$expected"; then
		printf '%s SHA-256 mismatch\n' "$file_name" >&2
		exit 1
	fi
	chmod 600 "$font_temp"
	sync -f "$font_temp"
	mv -f -- "$font_temp" "$final"
	font_temp=''
}

inter_ready=true
while read -r file_name expected; do
	sha_matches "$font_dir/$file_name" "$expected" || inter_ready=false
done <<'EOF'
Inter-Regular.ttf 529be850e06f62f8904f22bda77e45bde4834498fdbec4ff4201fa3177447a3a
Inter-Medium.ttf 6df88fcb83ac96582350f801355c6eff55f15710093e9627fb431caa40521151
Inter-SemiBold.ttf 2de533bda937a063c595b07c6bd9b70c8c5087d0649a1c8330f7ac11fcc05602
Inter-Bold.ttf e6c172fd8a2f957414a7a63ec8deb7f2aa239182394cfa5ee2ea6927c6194389
EOF

if [[ "$inter_ready" != true ]]; then
	download_archive inter_zip Inter-3.19 \
		'https://github.com/rsms/inter/releases/download/v3.19/Inter-3.19.zip' \
		'150ab6230d1762a57bebf35dfc04d606ff91598a31d785f7f100356ecdcc0032'
	publish_font "$inter_zip" 'Inter Hinted for Windows/Desktop/Inter-Regular.ttf' \
		'Inter-Regular.ttf' '529be850e06f62f8904f22bda77e45bde4834498fdbec4ff4201fa3177447a3a'
	publish_font "$inter_zip" 'Inter Hinted for Windows/Desktop/Inter-Medium.ttf' \
		'Inter-Medium.ttf' '6df88fcb83ac96582350f801355c6eff55f15710093e9627fb431caa40521151'
	publish_font "$inter_zip" 'Inter Hinted for Windows/Desktop/Inter-SemiBold.ttf' \
		'Inter-SemiBold.ttf' '2de533bda937a063c595b07c6bd9b70c8c5087d0649a1c8330f7ac11fcc05602'
	publish_font "$inter_zip" 'Inter Hinted for Windows/Desktop/Inter-Bold.ttf' \
		'Inter-Bold.ttf' 'e6c172fd8a2f957414a7a63ec8deb7f2aa239182394cfa5ee2ea6927c6194389'
fi

jetbrains_ready=true
while read -r file_name expected; do
	sha_matches "$font_dir/$file_name" "$expected" || jetbrains_ready=false
done <<'EOF'
JetBrainsMono-Regular.ttf a0bf60ef0f83c5ed4d7a75d45838548b1f6873372dfac88f71804491898d138f
JetBrainsMono-Medium.ttf 31c92d01a8a08528b718a43addf0ad3df0af2ca4b7b3290a452f70f358e14d3d
JetBrainsMono-SemiBold.ttf 1b3bfa1ed5665a4ce3f9feb68d2d4e40e70bf8b4b7d9a3edd418f321b4e166a0
JetBrainsMono-Bold.ttf 5590990c82e097397517f275f430af4546e1c45cff408bde4255dad142479dcb
EOF

if [[ "$jetbrains_ready" != true ]]; then
	download_archive jetbrains_zip JetBrainsMono-2.304 \
		'https://github.com/JetBrains/JetBrainsMono/releases/download/v2.304/JetBrainsMono-2.304.zip' \
		'6f6376c6ed2960ea8a963cd7387ec9d76e3f629125bc33d1fdcd7eb7012f7bbf'
	publish_font "$jetbrains_zip" 'fonts/ttf/JetBrainsMono-Regular.ttf' \
		'JetBrainsMono-Regular.ttf' 'a0bf60ef0f83c5ed4d7a75d45838548b1f6873372dfac88f71804491898d138f'
	publish_font "$jetbrains_zip" 'fonts/ttf/JetBrainsMono-Medium.ttf' \
		'JetBrainsMono-Medium.ttf' '31c92d01a8a08528b718a43addf0ad3df0af2ca4b7b3290a452f70f358e14d3d'
	publish_font "$jetbrains_zip" 'fonts/ttf/JetBrainsMono-SemiBold.ttf' \
		'JetBrainsMono-SemiBold.ttf' '1b3bfa1ed5665a4ce3f9feb68d2d4e40e70bf8b4b7d9a3edd418f321b4e166a0'
	publish_font "$jetbrains_zip" 'fonts/ttf/JetBrainsMono-Bold.ttf' \
		'JetBrainsMono-Bold.ttf' '5590990c82e097397517f275f430af4546e1c45cff408bde4255dad142479dcb'
fi

config_temp=$(mktemp "$root/.fonts.conf.XXXXXX")
cat >"$config_temp" <<EOF
<?xml version="1.0"?>
<!DOCTYPE fontconfig SYSTEM "urn:fontconfig:fonts.dtd">
<fontconfig>
  <dir>$font_dir</dir>
  <cachedir>$cache_dir</cachedir>
</fontconfig>
EOF
chmod 600 "$config_temp"
sync -f "$config_temp"
mv -f -- "$config_temp" "$config"
config_temp=''

inter_match=$(FONTCONFIG_FILE="$config" fc-match -f '%{file}' 'Inter:style=Regular')
jetbrains_match=$(FONTCONFIG_FILE="$config" fc-match -f '%{file}' 'JetBrains Mono:style=Regular')
if [[ "$inter_match" != "$font_dir/Inter-Regular.ttf" ]]; then
	printf 'isolated Inter resolution failed: %s\n' "$inter_match" >&2
	exit 1
fi
if [[ "$jetbrains_match" != "$font_dir/JetBrainsMono-Regular.ttf" ]]; then
	printf 'isolated JetBrains Mono resolution failed: %s\n' "$jetbrains_match" >&2
	exit 1
fi

printf '%s\n' "$config"
