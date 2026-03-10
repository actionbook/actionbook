# Actionbook-rs Performance Baseline

**Date**: 2026-03-10
**Branch**: `feat/benchmark-suite`
**Rust Version**: `rustc 1.83` (or current)
**Opt-level**: `z` (size-optimized)
**Test Machine**: macOS (Darwin 24.5.0)

---

## Executive Summary

**Key Findings**:
1. 🔴 **Critical**: Typed deserialization is **slower** than Value access for AX trees (3-6x)
2. ⚠️ **Warning**: Cache bytes path shows mixed results (save faster, load slower)
3. ✅ **Success**: CDP typed envelope shows 12% improvement for responses
4. 📊 **Baseline**: Base64 encoding established for 100KB-5MB payloads

**Impact on Master Plan**:
- Phase 2 (typed deserialization) assumptions **need revision**
- Must investigate why typed is slower (likely test artifact)
- Phase 1 (cache bytes) needs real-world validation

---

## 1. Cache I/O Benchmarks

**Test**: Session state serialization with 1000 items

### Save Operations

| Method | Time | vs String |
|--------|------|-----------|
| String (pretty) | 90.5 µs | baseline |
| Bytes (compact) | 77.1 µs | **-14.8% ✅** |

**Analysis**: Pretty-printing elimination shows expected gains.

### Load Operations

| Method | Time | vs String |
|--------|------|-----------|
| String | 63.2 µs | baseline |
| Bytes | 71.8 µs | **+13.6% ❌** |

**Analysis**: Unexpected regression. Possible causes:
1. Test data too small (I/O overhead dominates)
2. Need real SessionState structure (not mock)
3. File system caching effects

**Action**: Re-test with actual cache files before Phase 1 implementation.

---

## 2. Snapshot (A11y Tree) Benchmarks

**Test**: parse_ax_tree with synthetic AX trees

### Small Trees (10 nodes)

| Method | Time | vs Value |
|--------|------|----------|
| Value (current) | 1.93 µs | baseline |
| Envelope (2a) | 11.1 µs | **+475% 🔴** |
| Typed (2b) | 6.22 µs | **+222% 🔴** |

### Medium Trees (100 nodes)

| Method | Time | vs Value |
|--------|------|----------|
| Value (current) | 18.4 µs | baseline |
| Envelope (2a) | 109.5 µs | **+495% 🔴** |
| Typed (2b) | 60.5 µs | **+229% 🔴** |

### Large Trees (500 nodes) - **Most Important**

| Method | Time | vs Fair Value |
|--------|------|---------------|
| **Value (fair - with JSON parse)** | **551.1 µs** | **baseline** |
| Value (unfair - pre-parsed) | 97.6 µs | -82% (invalid comparison) |
| Envelope (2a) | 561.3 µs | +1.8% (no improvement) |
| **Typed (2b)** | **319.5 µs** | **-42% ✅✅✅** |

**✅ SUCCESS**: Typed deserialization is 42% faster than current approach!

**Analysis**:
1. **✅ FIXED**: Benchmark now includes JSON parsing in both paths for fair comparison
2. **✅ VALIDATED**: Typed deserialization is 42-45% faster across all sizes
3. **Phase 2a (Envelope)**: No improvement over fair Value (~+2%), should skip
4. **Phase 2b (Typed)**: Clear winner, exceeds 40% target

**Why Typed is Faster**:
- Single-pass deserialization (JSON → struct directly)
- No dynamic field lookups (`get()`, `as_str()`)
- No repeated allocations for intermediate Values
- Compile-time validated structure

**All Size Results**:

| Nodes | Value (fair) | Typed | Improvement |
|-------|-------------|-------|-------------|
| 10 | 11.3 µs | 6.2 µs | **-45%** ✅ |
| 100 | 109.7 µs | 62.4 µs | **-43%** ✅ |
| 500 | 551.1 µs | 319.5 µs | **-42%** ✅ |

**Action**:
- ✅ **GREEN LIGHT for Phase 2b**: Typed deserialization validated
- ⛔ **SKIP Phase 2a**: Envelope shows no benefit
- 📋 Proceed directly to Phase 2b (fully typed) implementation

---

## 3. CDP Message Benchmarks

**Test**: CDP protocol message parsing

### Response Messages

| Method | Time | vs Value |
|--------|------|----------|
| Value | 383 ns | baseline |
| Typed | 337 ns | **-12% ✅** |

### Event Messages

| Method | Time | vs Value |
|--------|------|----------|
| Value | 387 ns | baseline |
| Typed | 471 ns | **+22% ❌** |

### Error Messages

| Method | Time | vs Value |
|--------|------|----------|
| Value | 383 ns | (N/A) |
| Typed | 323 ns | **-16% ✅** |

**Analysis**:
- Response and Error: typed faster (expected)
- Event: typed slower (unexpected, needs investigation)
- Overall: mixed results, but improvements are small (<20%)

**Action**: Investigate Event parsing regression before Phase 2.

---

## 4. Base64 Encoding Benchmarks

**Test**: Screenshot/PDF encoding at realistic sizes

### Encoding Performance

| Size | Time | Throughput |
|------|------|------------|
| 100 KB | 83.4 µs | ~1.2 GB/s |
| 1 MB | 844 µs | ~1.2 GB/s |
| 5 MB | 4.54 ms | ~1.1 GB/s |

### Decoding Performance

| Size | Time | Throughput |
|------|------|------------|
| 100 KB | 195 µs | ~512 MB/s |
| 1 MB | 4.25 ms | ~235 MB/s |
| 5 MB | 9.99 ms | ~500 MB/s |

**Analysis**:
- Encoding: ~1.2 GB/s (stable across sizes)
- Decoding: ~250-500 MB/s (varies with size)
- Decode ~5x slower than encode (expected, due to validation)

**Action**: Baseline established. No optimization needed (not a bottleneck).

---

## 5. Binary Size & Startup (TODO)

**Not yet measured**. Need to run:
```bash
./scripts/bench-profile.sh
```

This will compare opt-level="z" vs "s" vs "3" for:
- Binary size
- `--help` startup time
- `config show --json` startup time
- `profile list --json` startup time

**Action**: Run after PR1 (dependency optimization) is merged.

---

## Conclusions & Next Steps

### ✅ Validated Assumptions

1. ✅ **Typed deserialization is 42-45% faster** - Phase 2b GREEN LIGHT
2. ✅ Cache bytes path is faster for **save** operations (-15%)
3. ✅ CDP typed envelope is faster for **responses** (-12%)
4. ✅ Base64 encoding is not a bottleneck

### ⚠️ Issues Requiring Investigation

1. ⚠️ Cache bytes path is slower for **load** operations (+13%) - needs real-world testing
2. ⚠️ CDP typed envelope is slower for **events** (+22%) - needs investigation
3. ⚠️ Envelope approach (Phase 2a) shows no benefit - should skip

### 🔧 Required Actions Before Phase 1/2

**Immediate (Blocking)**:
1. Fix snapshot benchmark to include JSON parsing in Value path
2. Re-run with fair comparison
3. Investigate cache load regression with real SessionState
4. Understand CDP Event parsing regression

**Optional (Nice to have)**:
1. Run `bench-profile.sh` for size/startup baseline
2. Add fixture files with real AX trees (from GitHub, LinkedIn)
3. Benchmark with different payload size distributions

### 📋 Decision Gate

**✅ APPROVED to proceed to Phase 2 (typed deserialization)**:
- [x] Snapshot benchmark shows typed is 42% faster ✅✅✅
- [x] Test artifacts eliminated (fair comparison established) ✅
- [x] Exceeds 40% target improvement ✅
- [ ] CDP Event regression still needs investigation (but doesn't block Phase 2)

**⚠️ CONDITIONAL approval for Phase 1 (cache bytes)**:
- [x] Save operation shows 14.8% improvement ✅ (close to 15% target)
- [ ] Load regression needs explanation
- [ ] Recommend: test with real SessionState before implementing

**📋 Recommended Priority**:
1. **Phase 2b** (Typed deserialization) - HIGH IMPACT, VALIDATED
2. **Phase 1** (Cache bytes) - MEDIUM IMPACT, NEEDS VALIDATION
3. **CDP Envelope** - LOW PRIORITY (mixed results)

---

## Raw Benchmark Data

Full results available in: `target/criterion/`

View HTML reports:
```bash
open target/criterion/report/index.html
```

Command to reproduce:
```bash
cargo bench --all
```

---

**Status**: ✅ Baseline established and validated
**Next Action**: Proceed to Phase 2b (Typed deserialization) - 42% improvement validated
**Update**: 2026-03-10 - Benchmark corrected, typed deserialization proven faster
