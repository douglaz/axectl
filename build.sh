#!/usr/bin/env bash
set -euo pipefail

# Build script for axectl - produces static musl binary by default
#
# Usage:
#   ./build.sh          # Build static release binary
#   ./build.sh --dev    # Build development version (faster)
#   ./build.sh --test   # Build and run tests

echo "🔧 Building axectl..."

case "${1:-}" in
  --dev)
    echo "📦 Building development version (faster, non-static)"
    cargo build --target x86_64-unknown-linux-gnu
    echo "✅ Development build complete: ./target/x86_64-unknown-linux-gnu/debug/axectl"
    ;;
  --test)
    echo "🧪 Building and testing static binary"
    cargo build --release --target x86_64-unknown-linux-musl
    cargo test --release --target x86_64-unknown-linux-musl
    echo "✅ Build and tests complete: ./target/x86_64-unknown-linux-musl/release/axectl"
    # Verify it's static
    echo "📋 Binary info:"
    file ./target/x86_64-unknown-linux-musl/release/axectl
    ldd ./target/x86_64-unknown-linux-musl/release/axectl || echo "✅ Static binary confirmed (no dynamic dependencies)"
    ;;
  *)
    echo "📦 Building static release binary"
    cargo build --release --target x86_64-unknown-linux-musl
    echo "✅ Static build complete: ./target/x86_64-unknown-linux-musl/release/axectl"
    echo "📋 Binary info:"
    file ./target/x86_64-unknown-linux-musl/release/axectl
    ldd ./target/x86_64-unknown-linux-musl/release/axectl || echo "✅ Static binary confirmed (no dynamic dependencies)"
    ;;
esac

echo ""
echo "🚀 Ready to use! Try: ./target/*/release/axectl --help"