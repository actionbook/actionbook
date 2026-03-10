#!/bin/bash
# Benchmark different opt-levels
#
# Usage: ./scripts/bench-profile.sh

set -e

PROFILES=("z" "s" "3")
RESULTS_DIR="target/bench-results"
mkdir -p "$RESULTS_DIR"

# Check for hyperfine, fallback to manual timing if not available
if command -v hyperfine &> /dev/null; then
    BENCH_CMD="hyperfine --warmup 20 --runs 50"
    HAS_HYPERFINE=true
else
    echo "Warning: hyperfine not found, using fallback timing method"
    echo "Install with: cargo install hyperfine"
    HAS_HYPERFINE=false
fi

for profile in "${PROFILES[@]}"; do
    echo "========================================"
    echo "Testing opt-level=$profile"
    echo "========================================"

    # Build
    echo "Building with opt-level=$profile..."
    RUSTFLAGS="-C opt-level=$profile" cargo build --release

    # Measure binary size
    SIZE=$(ls -lh target/release/actionbook | awk '{print $5}')
    echo "Binary size: $SIZE"
    echo "$SIZE" > "$RESULTS_DIR/size-$profile.txt"

    # Measure startup time
    if [ "$HAS_HYPERFINE" = true ]; then
        echo "Benchmarking startup time..."
        $BENCH_CMD \
            --export-markdown "$RESULTS_DIR/startup-$profile.md" \
            'target/release/actionbook --help' \
            'target/release/actionbook config show --json' \
            'target/release/actionbook profile list --json'
    else
        echo "Benchmarking startup time (fallback)..."
        for cmd in "--help" "config show --json" "profile list --json"; do
            echo "  actionbook $cmd"
            total=0
            for i in $(seq 1 50); do
                start=$(date +%s%N)
                target/release/actionbook $cmd > /dev/null 2>&1
                end=$(date +%s%N)
                elapsed=$(( (end - start) / 1000000 ))
                total=$(( total + elapsed ))
            done
            mean=$(( total / 50 ))
            echo "    Mean: ${mean}ms (50 runs)" | tee -a "$RESULTS_DIR/startup-$profile.txt"
        done
    fi

    echo
done

echo "========================================"
echo "Results saved to: $RESULTS_DIR"
echo "========================================"
ls -lh "$RESULTS_DIR"
