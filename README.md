# normal-path

[![Crates.io](https://img.shields.io/crates/v/normal-path.svg)](https://crates.io/crates/normal-path)
[![Documentation](https://docs.rs/normal-path/badge.svg)](https://docs.rs/normal-path)
[![License](https://img.shields.io/crates/l/normal-path.svg)](https://crates.io/crates/normal-path)

A Rust library to ensure paths are normalized without touching the filesystem.

This library provides two types, `NormpathBuf` and `Normpath` (akin to `PathBuf` and `Path`), for working with normalized paths abstractly.
Similar to the standard library, these types are thin wrappers around `OsString` and `OsStr` that represent normalized paths according to the local platform's path syntax.

## Features

- **No I/O**: Normalizes paths purely on the string level, without touching the filesystem.
- **Cross-platform**: Handles Unix and Windows path semantics correctly.
- **Type-checked**: Ensures paths are absolute, canonical, and free of parent directory components (`..`).
- **Normalization**: Can normalize non-canonical patterns (like `//`, `./`, trailing slashes, and on Windows, forward slashes and lowercase drive letters) into their canonical forms.
- **Serde support**: Optional `serde` feature for serialization and deserialization.

### Example

```rust
use normal_path::{Normpath, NormpathBuf};

// Validate an already normalized path
let path = Normpath::validate("/foo/bar").unwrap();

// Normalize a path with non-canonical patterns
let normalized = NormpathBuf::normalize("/foo/./bar//".into()).unwrap();
assert_eq!(&normalized, "/foo/bar");
```

## Normalization Rules

In general, a path is considered normalized if and only if:

1. It is absolute, i.e., independent of the current directory.
2. It is canonical, meaning it does not contain any pattern that is considered non-canonical and has a corresponding canonical form depending on the platform.
3. It does not contain any parent directory component (`..`).

A normalized path is not necessarily the ["real"][1] path to the filesystem object it denotes, which may not even exist.
Instead, it defines a path unambiguously on the string level, unaffected by the state of the filesystem.

### Canonicality

These patterns are considered non-canonical on both Unix and Windows:
1. Multiple consecutive slashes (`foo//bar`).
2. Trailing slashes (`foo/bar/`).
3. Current directory components (`foo/.` or `./foo`).

Specifically on Windows, the following patterns are also considered non-canonical:
1. Forward slashes (`D:\foo/bar` or `//./COM11`).
2. Lowercase drive letters (`d:\`).
3. Parent directory components (that can be normalized).

### Handling Parent Directories

On Unix, any parent component is an error, since it is impossible to eliminate them without filesystem access.

Windows, on the other hand, collapses parent components lexically *before* walking the filesystem (unless the path starts with `\\?\`).
Therefore, it is only a *hard error* if a parent component points outside of the base directory, like `C:\..`.

## License

Licensed under either of

 - Apache License, Version 2.0 ([LICENSE-APACHE](LICENSE-APACHE) or http://www.apache.org/licenses/LICENSE-2.0)
 - MIT license ([LICENSE-MIT](LICENSE-MIT) or http://opensource.org/licenses/MIT)

at your option.

[1]: https://doc.rust-lang.org/std/fs/fn.canonicalize.html
