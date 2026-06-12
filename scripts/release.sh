#!/bin/sh
# Publish a new version of nr.
#
# Usage: ./scripts/release.sh 1.2.0
#
# Bumps Cargo.toml, builds, runs the test suite, commits, tags vX.Y.Z,
# and pushes. The Release GitHub Action then builds binaries for all
# platforms and publishes the GitHub release.
set -e

VERSION="$1"
if [ -z "$VERSION" ]; then
    echo "Usage: ./scripts/release.sh <version>   (e.g. 1.2.0)"
    exit 1
fi

if ! echo "$VERSION" | grep -Eq '^[0-9]+\.[0-9]+\.[0-9]+$'; then
    echo "Error: version must be X.Y.Z, got '$VERSION'"
    exit 1
fi

BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [ "$BRANCH" != "main" ]; then
    echo "Error: releases must be cut from main (currently on $BRANCH)"
    exit 1
fi

if [ -n "$(git status --porcelain)" ]; then
    echo "Error: working tree is not clean. Commit or stash first."
    exit 1
fi

git pull --ff-only

if git rev-parse "v$VERSION" >/dev/null 2>&1; then
    echo "Error: tag v$VERSION already exists"
    exit 1
fi

echo "Releasing v$VERSION..."

# Bump Cargo.toml (the only `version =` line is the package version)
sed -i.bak "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml
rm Cargo.toml.bak

# Rebuild (also refreshes Cargo.lock with the new version) and verify
cargo build --release
./test.sh

git commit -am "v$VERSION"
# -m makes the tag annotated, required when tag.gpgsign is enabled
git tag -m "v$VERSION" "v$VERSION"
git push origin main --tags

echo ""
echo "Pushed v$VERSION. The Release workflow is building binaries."

# Best effort: watch the workflow if gh is installed
if command -v gh >/dev/null 2>&1; then
    echo "Waiting for the workflow to start..."
    RUN_ID=""
    for _ in 1 2 3 4 5 6; do
        sleep 5
        RUN_ID=$(gh run list --workflow=release.yml --limit 1 \
            --json databaseId,headBranch,status \
            -q '.[] | select(.status != "completed") | .databaseId' | head -1)
        [ -n "$RUN_ID" ] && break
    done
    if [ -n "$RUN_ID" ]; then
        gh run watch "$RUN_ID" --exit-status
        echo "Release published: https://github.com/dawsbot/nr/releases/tag/v$VERSION"
    else
        echo "Could not find the run. Check https://github.com/dawsbot/nr/actions"
    fi
else
    echo "Watch progress at https://github.com/dawsbot/nr/actions"
fi
