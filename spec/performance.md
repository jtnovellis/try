# Performance Specification

## Overview

The `try` tool should feel instant even with hundreds of directories. This document specifies performance requirements and design patterns.

## Directory Scanning

### Single Pass Loading

- Directory list is loaded **once** at startup
- Subsequent operations (filtering, sorting) work on the cached list
- List is only reloaded after mutations (delete, create)

### Efficient Metadata Retrieval

- Use single syscall per directory to get modification time
- Prefer `stat()` over `readdir()` + `stat()` when possible
- Cache modification times in memory

### Platform-Specific Optimizations

The Rust implementation uses:
- `std::fs::read_dir` for portable directory listing
- `std::fs::symlink_metadata` for lstat (symlink-aware metadata)
- `std::fs::metadata` for stat (follows symlinks for is_dir check)
- Single-pass entry loading with in-memory caching

## Fuzzy Matching

### Forward-Only Algorithm

The fuzzy matcher must be **O(n×m)** where:
- n = length of query
- m = length of directory name

**Requirements:**
- Single forward pass through both strings
- No backtracking or recursion
- Early termination on mismatch

### Scoring Algorithm

```
For each character in query:
  Scan forward in target for match
  If found:
    score += base_points
    score += proximity_bonus / sqrt(gap + 1)
  Else:
    return 0 (no match)
```

The proximity bonus rewards consecutive matches without requiring backtracking.

## Rendering

### Single-Buffer Frame Rendering

- Build complete frame in a `String` buffer
- Flush entire buffer to stderr in a single `write_all` call
- Avoids visible screen tearing

### Incremental Updates

When only the selection changes:
- Full re-render of the frame each keystroke (simple, reliable at this scale)
- At directory scale (hundreds), full re-render is still < 16ms

### ANSI Direct Emission

- No intermediate token layer — styling functions emit ANSI codes directly
- Color check is a single `AtomicBool` load (O(1))
- Unicode width via simplified char-width table (no regex, no complex parsing)

## Memory Usage

### String Handling

- Use Rust's `String` and `&str` slices to avoid unnecessary copies
- Pre-allocate `String` buffers for rendering with `String::new()`
- UTF-8 character iteration via `.chars()` for Unicode-aware operations
- Byte-level operations for ASCII fast paths (e.g., date prefix check)

### Data Structures

- Directory list: `Vec<DirEntry>` (contiguous, cache-friendly iteration)
- Match results: `Vec<MatchResult>` sorted by score
- No complex tree structures for small datasets
- Highlight positions: `Vec<usize>` with `HashSet` for lookup during rendering

## Benchmarks

Target performance (rough guidelines):

| Operation | Target |
|-----------|--------|
| Startup + first render | < 50ms |
| Keystroke to screen update | < 16ms (60fps) |
| Fuzzy filter 1000 entries | < 10ms |
| Directory scan 1000 entries | < 100ms |

## Anti-Patterns to Avoid

1. **Multiple directory scans** - Never re-read filesystem during filtering
2. **Backtracking matchers** - No recursive fuzzy matching
3. **Regex for styling** - Use direct ANSI function calls in `src/ansi.rs`
4. **Per-character rendering** - Always batch screen updates via single `write_all`
5. **Sorting during filter** - Sort once, filter in-place
6. **String concatenation in loops** - Use `String::push_str` / `format!`
