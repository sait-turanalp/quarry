#!/usr/bin/env bash
set -euo pipefail

echo "Testing Release Workflow Locally"
echo "================================="
echo ""

# Test 1: Version extraction
echo "1. Testing version extraction..."
version=$(grep -m1 '^version = ' Cargo.toml | cut -d'"' -f2)

if [[ -z "$version" ]]; then
  echo "Error: no version found in Cargo.toml"
  exit 1
fi

echo "✓ Version: $version"
echo ""

# Test 2: Build commands
echo "2. Testing build commands..."
target=$(rustc -vV | grep host | cut -d' ' -f2)

echo "   Building full variant (--all-features)..."
cargo build --release --locked --target $target --all-features

echo "   Building slim variant (no features)..."
cargo build --release --locked --target $target

echo "✓ Both builds successful"
echo ""

# Test 3: Packaging
echo "3. Testing packaging..."
test_dir="/tmp/codanna-release-test-$$"
mkdir -p "$test_dir"

bin="target/$target/release/codanna"
dst_full="$test_dir/codanna-$version-macos-arm64"
dst_slim="$test_dir/codanna-$version-macos-arm64-slim"

# Package full
mkdir "$dst_full"
cp "$bin" "$dst_full/"
cp LICENSE "$dst_full/"
tar -cJf "$dst_full.tar.xz" -C "$test_dir" "$(basename "$dst_full")"

# Package slim
mkdir "$dst_slim"
cp "$bin" "$dst_slim/"
cp LICENSE "$dst_slim/"
tar -cJf "$dst_slim.tar.xz" -C "$test_dir" "$(basename "$dst_slim")"

echo "✓ Packaging successful"
ls -lh "$test_dir"/*.tar.xz
echo ""

# Test 4: Checksums
echo "4. Testing checksum generation..."
(cd "$test_dir" && sha256sum *.tar.xz | tee SHA256SUMS)
(cd "$test_dir" && sha512sum *.tar.xz | tee SHA512SUMS)

echo "✓ Checksums generated"
echo ""

# Test 5: Manifest
echo "5. Testing manifest generation..."
(cd "$test_dir" && jq -nc \
  --arg version "$version" \
  --arg files "$(ls *.tar.xz 2>/dev/null | tr '\n' ' ')" \
  '{
    app: "codanna",
    version: $version,
    artifacts: ($files | split(" ") | map(select(length > 0)) | map({
      name: .,
      variant: (if test("-slim") then "slim" else "full" end),
      platform: (capture("codanna-[^-]+-(?<p>[^.]+)").p | sub("-slim$"; "")),
      features: (if test("-slim") then [] else ["http-server", "https-server"] end)
    }))
  }' | tee dist-manifest.json)

echo "✓ Manifest generated"
echo ""

# Summary
echo "================================="
echo "All workflow steps verified!"
echo ""
echo "Test artifacts in: $test_dir"
echo "To inspect: ls -la $test_dir"
echo ""
echo "Cleanup with: rm -rf $test_dir"
