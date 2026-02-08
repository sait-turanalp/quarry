#!/usr/bin/env bash
# fix.sh — macOS/POSIX-safe
set -euo pipefail

REPO_ROOT="$(git rev-parse --show-toplevel 2>/dev/null || pwd)"
cd "$REPO_ROOT"

# Collect .rs files safely (handles spaces/newlines)
files=()
while IFS= read -r -d '' f; do
  files+=("$f")
done < <(git ls-files -z '*.rs')

if [ "${#files[@]}" -eq 0 ]; then
  echo "No .rs files found"
  exit 0
fi

# Normalize line endings and strip trailing whitespace
for f in "${files[@]}"; do
  # Remove CR and trailing spaces/tabs
  perl -0777 -i -pe 's/\r$//mg; s/[ \t]+$//mg' "$f"

  # Ensure file ends with exactly one newline
  if [ -s "$f" ]; then
    # Append newline if last byte isn’t "\n"
    if ! tail -c 1 "$f" | grep -q $'\n'; then
      printf '\n' >> "$f"
    fi
    # Collapse multiple trailing newlines to one
    perl -0777 -i -pe 's/\n+\z/\n/' "$f"
  fi
done

echo "Running cargo fmt..."
cargo fmt --all

echo "Done."