#!/usr/bin/env bash
# Copyright 2026 the Runebender Authors
# SPDX-License-Identifier: Apache-2.0 OR MIT
#
# Open an upstream pull request from a file in docs/upstream/.
# The owner runs this by hand. It refuses to run without a terminal, so an agent cannot use it.
# See docs/upstream/linebender/README.md for the process this script enforces.

set -euo pipefail

fail() {
    echo "error: $*" >&2
    exit 1
}

[ $# -eq 1 ] || fail "usage: $0 <docs/upstream/.../file.md>"
file=$1
[ -f "$file" ] || fail "no such file: $file"
[ -t 0 ] && [ -t 1 ] || fail "run this in an interactive terminal; it is not for agents or scripts"
command -v gh >/dev/null || fail "the gh command is not installed"

# Read one field from the header block between the first two '---' lines.
field() {
    awk -v key="$1" '
        /^---$/ { block++; next }
        block == 1 && index($0, key ":") == 1 {
            value = substr($0, length(key) + 2)
            gsub(/^[ \t]+|[ \t]+$/, "", value)
            print value
            exit
        }' "$file"
}

# Print the lines between two marker comments.
section() {
    awk -v start="<!-- $1:start -->" -v end="<!-- $1:end -->" '
        $0 == end { inside = 0 }
        inside { print }
        $0 == start { inside = 1 }' "$file"
}

upstream=$(field upstream_repository)
base=$(field base_branch)
head_repository=$(field head_repository)
head_branch=$(field head_branch)
reviewed=$(field owner_reviewed_commit)
disclosure=$(field llm_disclosure)

[ -n "$upstream" ] || fail "upstream_repository is empty"
[ -n "$base" ] || fail "base_branch is empty"
[ -n "$head_repository" ] || fail "head_repository is empty"
[ -n "$head_branch" ] || fail "head_branch is empty"
[ -z "$(field pull_request)" ] || fail "this file already records a pull request"
[ "$(field owner_decided_design)" = "yes" ] || fail "owner_decided_design is not yes"
[ "$(field owner_wrote_text)" = "yes" ] || fail "owner_wrote_text is not yes"
case "$disclosure" in
    included | not-needed) ;;
    *) fail "llm_disclosure must be 'included' or 'not-needed'" ;;
esac
[ -n "$reviewed" ] || fail "owner_reviewed_commit is empty"

title=$(section title)
body=$(section body)
[ -n "$title" ] || fail "the title is empty"
[ "$(printf '%s\n' "$title" | wc -l | tr -d ' ')" = "1" ] || fail "the title must be one line"
[ -n "$body" ] || fail "the description is empty"
if printf '%s\n%s\n' "$title" "$body" | grep -q 'TODO(owner)'; then
    fail "a TODO(owner) placeholder remains in the title or the description"
fi

remote_head=$(git ls-remote "https://github.com/$head_repository" "refs/heads/$head_branch" | cut -f1)
[ -n "$remote_head" ] || fail "branch $head_branch is not pushed to $head_repository"
case "$remote_head" in
    "$reviewed"*) ;;
    *) fail "the pushed branch is at $remote_head, but the owner reviewed $reviewed; review the new commits first" ;;
esac

owner=${head_repository%%/*}
body_file=$(mktemp)
trap 'rm -f "$body_file"' EXIT
printf '%s\n' "$body" > "$body_file"

echo
echo "Repository:  $upstream  (base: $base)"
echo "Branch:      $owner:$head_branch at $remote_head"
echo "Disclosure:  $disclosure"
echo "Title:       $title"
echo "---------------------------------------------------------------- description"
cat "$body_file"
echo "----------------------------------------------------------------------------"
echo
echo "You wrote this title and description yourself, and you have read the full diff."
printf 'Type the repository name (%s) to open a draft pull request: ' "$upstream"
read -r answer
[ "$answer" = "$upstream" ] || fail "not confirmed; nothing was sent"

gh pr create --repo "$upstream" --base "$base" --head "$owner:$head_branch" \
    --title "$title" --body-file "$body_file" --draft

echo
echo "Record the link in the 'pull_request' field and the Record section of $file."
