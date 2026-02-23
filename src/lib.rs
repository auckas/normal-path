#![cfg_attr(docsrs, feature(doc_cfg))]
//! A library to ensure paths are normalized without touching the filesystem.
//!
//! This library provides two types, [`NormpathBuf`] and [`Normpath`] (akin to
//! [`PathBuf`] and [`Path`]), for working with normalized paths abstractly.
//! Similar to the standard library, these types are thin wrappers around
//! [`OsString`] and [`OsStr`] that represent normalized paths according to the
//! local platform's path syntax.
//!
//! In general, a path is considered normalized if and only if:
//!
//! 1. It is [absolute][1], i.e. independent of the current directory.
//!
//! 2. It is canonical, meaning it does not contain any pattern that is
//!    considered non-canonical and has a corresponding canonical form
//!    depending on the platform.
//!
//! 3. It does not contain any parent directory component (`..`).
//!
//! A normalized path is not necessarily the ["real"][2] path to the filesystem
//! object it denotes, which may not even exist. Instead, it defines a path
//! unambiguously on the string level, unaffected by the state of the
//! filesystem.
//!
//! All [`Normpath`] slices and [`NormpathBuf`] instances are guaranteed to
//! uphold these invariants, and can be obtained by validating an existing path,
//! or even further, by normalizing non-canonical patterns found in a path into
//! their canonical forms.
//!
//! This library never touches the filesystem, and will never attempt to
//! alter a path in a way that might change the object it denotes, such as
//! eliminating parent directory components on Unix. Since filesystem access is
//! inevitable, [`std::fs`] or third-party crates should be used in order to
//! resolve such paths.
//!
//! # Canonicality
//!
//! These patterns are considered non-canonical on both Unix and Windows:
//! 1. Multiple consecutive slashes (`foo//bar`).
//! 2. Trailing slashes (`foo/bar/`).
//! 3. Current directory components (`foo/.` or `./foo`).
//!
//! Specifically on Windows, the following patterns are also considered
//! non-canonical:
//! 1. Forward slashes (`D:\foo/bar` or `//./COM11`).
//! 2. Lowercase drive letters (`d:\`).
//! 3. Parent directory components (that can be normalized). See more on that
//!    below.
//!
//! All of these patterns can be normalized into their canonical forms if
//! desired.
//!
//! # Handling Parent Directories
//!
//! On Unix, any parent component is an error, since it is impossible to
//! eliminate them without filesystem access.
//!
//! Windows, on the other hand, collapses parent components lexically *before*
//! walking the filesystem (unless the path starts with `\\?\`). Therefore,
//! it is only a *hard error* if a parent component points outside of the base
//! directory, like `C:\..`.
//!
//! This leads to a subtlety on Windows about which error a parent directory
//! component should raise during validation:
//!
//! 1. If the parent component can be normalized away, e.g. `C:\foo\..`, then
//!    it is only an issue on the canonicality of the path.
//!
//! 2. But if the parent component points outside of the base directory, e.g.
//!    `C:\..`, then it is instead a parent component error similar to Unix.
//!
//! It worth noting that no parent directory component is *ever* allowed for a
//! path to be considered normalized. It is *always* an error; the distinction
//! is merely about which category the error falls into.
//!
//! # Notes on Windows
//!
//! This library always assumes case-sensitivity as Windows can be
//! [case-sensitive][3] with respect to filesystem paths.
//!
//! Verbatim paths (starting with `\\?\`) are *always* considered normalized.
//! Consider using third-party crates like [`dunce`][4] to remove the verbatim
//! prefix if that is not desired.
//!
//! # Feature Flags
//! - `serde`: adds support for serialization and deserialization.
//!
//! [1]: std::path::Path::is_absolute
//! [2]: std::fs::canonicalize
//! [3]: https://learn.microsoft.com/en-us/windows/wsl/case-sensitivity
//! [4]: https://crates.io/crates/dunce
//!
//! [`OsStr`]: std::ffi::OsStr
//! [`OsString`]: std::ffi::OsString
//! [`Path`]: std::path::Path
//! [`PathBuf`]: std::path::PathBuf

mod draw;
mod trivial;
pub use trivial::{ConvertError, Error, Normpath, NormpathBuf};

mod imp;

mod public;
pub use public::*;
