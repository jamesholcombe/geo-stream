#!/usr/bin/env bash
# Usage: scripts/bump.sh [patch|minor|major]
# version.txt is the source of truth. This script bumps it, then syncs
# Cargo.toml (via cargo set-version) and package.json to match.
# Requires: cargo-edit  →  cargo install cargo-edit
set -euo pipefail

BUMP=${1:?Usage: scripts/bump.sh [patch|minor|major]}

# Stash any uncommitted changes so they don't end up in the bump commit
STASHED=false
if ! git diff --quiet || ! git diff --cached --quiet; then
  git stash push -m "pre-bump stash"
  STASHED=true
fi

# Always pop the stash on exit (success or failure)
pop_stash() {
  if [ "$STASHED" = true ]; then
    git stash pop
  fi
}
trap pop_stash EXIT

CURRENT=$(cat version.txt | tr -d '[:space:]')
IFS='.' read -r MAJOR MINOR PATCH <<< "$CURRENT"

case "$BUMP" in
  patch) PATCH=$((PATCH + 1)) ;;
  minor) MINOR=$((MINOR + 1)); PATCH=0 ;;
  major) MAJOR=$((MAJOR + 1)); MINOR=0; PATCH=0 ;;
  *) echo "Unknown bump type: $BUMP (use patch, minor, or major)"; exit 1 ;;
esac

NEW_VERSION="$MAJOR.$MINOR.$PATCH"

# Update version.txt
echo "$NEW_VERSION" > version.txt

# Sync Cargo workspace (updates Cargo.toml + Cargo.lock)
cargo set-version "$NEW_VERSION"

# Sync package.json
node -e "
  const fs = require('fs');
  const path = 'crates/adapters/napi/package.json';
  const pkg = JSON.parse(fs.readFileSync(path, 'utf8'));
  pkg.version = '$NEW_VERSION';
  fs.writeFileSync(path, JSON.stringify(pkg, null, 2) + '\n');
"

echo "Bumped $CURRENT → $NEW_VERSION"

git add version.txt Cargo.toml Cargo.lock \
        crates/engine/Cargo.toml crates/state/Cargo.toml \
        crates/spatial/Cargo.toml crates/adapters/stdin-stdout/Cargo.toml \
        crates/adapters/napi/Cargo.toml crates/adapters/napi/package.json \
        crates/cli/Cargo.toml
git commit -m "chore: bump version to v$NEW_VERSION"
git tag "v$NEW_VERSION"

echo "Tagged v$NEW_VERSION locally — run 'git push && git push --tags' to release"
