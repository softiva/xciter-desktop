#!/bin/sh
# Tests for scripts/tag-version, scripts/version-info and scripts/set-version.  Run: sh scripts/tests/version.sh
set -u
here=$(cd "$(dirname "$0")/.." && pwd)
failures=0

pass() { printf 'ok   %s\n' "$1"; }
fail() { printf 'FAIL %s\n' "$1"; failures=$((failures + 1)); }

# tag-version: accepted tags and the outputs they give.
expect_tag() {
    tag=$1 expected=$2
    if actual=$("$here/tag-version" "$tag" 2>&1 | tr '\n' ' ') && [ "$actual" = "$expected" ]; then
        pass "tag-version $tag"
    else
        fail "tag-version $tag: got '$actual', expected '$expected'"
    fi
}
expect_tag v1.0.0 'version=1.0.0 prerelease=false msix=1.0.0.0 msi=true '
expect_tag v1.2.3 'version=1.2.3 prerelease=false msix=1.2.3.0 msi=true '
expect_tag v10.20.30 'version=10.20.30 prerelease=false msix=10.20.30.0 msi=true '
expect_tag v1.0.0-beta.1 'version=1.0.0-beta.1 prerelease=true msix=1.0.0.0 msi=false '
expect_tag v1.0.0-rc.2 'version=1.0.0-rc.2 prerelease=true msix=1.0.0.0 msi=false '
expect_tag v0.1.1-rc.1 'version=0.1.1-rc.1 prerelease=true msix=0.1.1.0 msi=false '

# tag-version: rejected tags.
for tag in v1.0b v1.0 1.0.0 v01.0.0 v1.0.0+build.1 v1.0.0- v1.0.0-beta..1 vx.y.z ''; do
    if "$here/tag-version" "$tag" >/dev/null 2>&1; then
        fail "tag-version rejects '$tag'"
    else
        pass "tag-version rejects '$tag'"
    fi
done

# A scratch copy of the three version files.
scratch=$(mktemp -d)
trap 'rm -rf "$scratch"' EXIT
mkdir -p "$scratch/app" "$scratch/core"
cp "$here/../app/tauri.conf.json" "$here/../app/Cargo.toml" "$scratch/app/"
cp "$here/../core/Cargo.toml" "$scratch/core/"

# set-version writes all three, and only the version lines.
if ROOT=$scratch SKIP_LOCK=1 "$here/set-version" 2.3.4-rc.1 >/dev/null \
    && grep -q '^  "version": "2.3.4-rc.1",$' "$scratch/app/tauri.conf.json" \
    && grep -q '^version = "2.3.4-rc.1"$' "$scratch/app/Cargo.toml" \
    && grep -q '^version = "2.3.4-rc.1"$' "$scratch/core/Cargo.toml" \
    && [ "$(grep -c '2.3.4-rc.1' "$scratch/core/Cargo.toml")" = 1 ]; then
    pass "set-version writes the three files"
else
    fail "set-version writes the three files"
fi
if ROOT=$scratch SKIP_LOCK=1 "$here/set-version" 1.0 >/dev/null 2>&1; then
    fail "set-version rejects a non-SemVer version"
else
    pass "set-version rejects a non-SemVer version"
fi

# version-info: a matching tag, a mismatched tag, a branch build, and files that disagree.
if out=$(ROOT=$scratch "$here/version-info" refs/tags/v2.3.4-rc.1 abcdef1234567) \
    && printf '%s\n' "$out" | grep -qx 'is_tag=true' && printf '%s\n' "$out" | grep -qx 'label=2.3.4-rc.1' \
    && printf '%s\n' "$out" | grep -qx 'prerelease=true'; then
    pass "version-info accepts a matching tag"
else
    fail "version-info accepts a matching tag"
fi
if ROOT=$scratch "$here/version-info" refs/tags/v2.3.5 abcdef1 >/dev/null 2>&1; then
    fail "version-info rejects a tag that differs from the code"
else
    pass "version-info rejects a tag that differs from the code"
fi
if out=$(ROOT=$scratch "$here/version-info" refs/heads/main abcdef1234567) \
    && printf '%s\n' "$out" | grep -qx 'label=2.3.4-rc.1+abcdef1' && printf '%s\n' "$out" | grep -qx 'is_tag=false' \
    && printf '%s\n' "$out" | grep -qx 'version=2.3.4-rc.1'; then
    pass "version-info labels a branch build with the short commit"
else
    fail "version-info labels a branch build with the short commit"
fi
sed 's/^version = ".*"$/version = "9.9.9"/' "$scratch/core/Cargo.toml" >"$scratch/core/Cargo.toml.new" && mv "$scratch/core/Cargo.toml.new" "$scratch/core/Cargo.toml"
if ROOT=$scratch "$here/version-info" refs/heads/main abcdef1 >/dev/null 2>&1; then
    fail "version-info rejects files that disagree"
else
    pass "version-info rejects files that disagree"
fi

echo "$failures failed"
[ "$failures" = 0 ]
