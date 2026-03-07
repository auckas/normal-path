#![cfg(windows)]

use std::{
    borrow::Cow,
    ffi::{OsStr, OsString},
    mem,
    path::{Component, Path, PathBuf, Prefix, PrefixComponent},
};

use super::super::trivial::{
    cast_ref_unchecked, ConvertError as Ec, Error as E, Normpath, NormpathBuf,
};

macro_rules! len {
    ($path:expr) => {
        $path.as_os_str().len()
    };
}

fn prefix_of(path: &Path) -> Option<(PrefixComponent<'_>, &Path)> {
    let bytes = path.as_os_str().as_encoded_bytes();
    let prefix = match bytes.get(..2)? {
        [b'a'..=b'z' | b'A'..=b'Z', b':'] | [b'/' | b'\\', b'/' | b'\\'] => {
            let mut components = path.components();
            match components.next()? {
                Component::Prefix(p) => p,
                _ => return None,
            }
        }
        _ => return None,
    };

    // SAFETY: the tail slice starts immediately after a valid UTF-8 substring,
    // so it is always a valid OS string.
    let raw = &bytes[len!(prefix)..];
    let tail = unsafe { OsStr::from_encoded_bytes_unchecked(raw) };

    Some((prefix, Path::new(tail)))
}

fn no_prefix(path: &Path) -> bool {
    prefix_of(path).is_none()
}

fn is_verbatim(path: &Path) -> bool {
    match prefix_of(path) {
        Some((prefix, _)) => prefix.kind().is_verbatim(),
        None => false,
    }
}

fn is_phony_root(path: &Path) -> bool {
    use Prefix::*;
    match prefix_of(path) {
        Some((prefix, tail)) => match prefix.kind() {
            DeviceNS(_) | UNC(_, _) => tail.as_os_str() == "\\",
            _ => false,
        },
        None => false,
    }
}

#[cfg(debug_assertions)]
fn assert_no_prefix(path: &Path) {
    if let Some((prefix, _)) = prefix_of(path) {
        panic!("unexpected prefix: {prefix:?}")
    }
}

#[cfg(not(debug_assertions))]
#[inline(always)]
fn assert_no_prefix(_: &Path) {}

#[cfg(debug_assertions)]
fn assert_unixlike_relative(path: &Path) {
    assert_no_prefix(path);
    assert!(!path.has_root());
}

#[cfg(not(debug_assertions))]
#[inline(always)]
fn assert_unixlike_relative(_: &Path) {}

#[derive(Debug)]
struct Searcher<'a> {
    haystack: &'a [u8],
    level: u32,
    err_slash: bool,
}

fn search_next(s: &mut Searcher<'_>) -> Option<E> {
    enum Seen {
        Empty,
        Plain,
        Slash,
        SlashDot,
        SlashDotDot,
    }
    use Seen::*;

    if s.err_slash {
        s.err_slash = false;
        return Some(E::NotCanonical);
    }

    let mut byte = s.haystack.first().copied();
    let mut seen = match byte {
        None | Some(b'/' | b'\\') => Empty,
        _ => Slash,
    };

    loop {
        match (seen, byte) {
            (Slash | SlashDot, None | Some(b'/' | b'\\')) => {
                s.err_slash = false; // collapse two errors into one
                return Some(E::NotCanonical);
            }
            (SlashDotDot, None | Some(b'/' | b'\\')) => {
                if let Some(new) = s.level.checked_sub(1) {
                    s.err_slash = false;
                    s.level = new;
                    return Some(E::NotCanonical);
                } else {
                    s.err_slash |= matches!(byte, Some(b'/'));
                    return Some(E::ContainsParent);
                }
            }
            (Empty | Plain, None) => {
                return None;
            }
            (Plain, Some(byte @ (b'/' | b'\\'))) => {
                s.level += 1;
                match byte {
                    b'/' => return Some(E::NotCanonical),
                    _ => seen = Slash,
                }
            }
            (Empty, Some(b'/' | b'\\')) => seen = Slash,
            (Slash, Some(b'.')) => seen = SlashDot,
            (SlashDot, Some(b'.')) => seen = SlashDotDot,
            (_, Some(_)) => seen = Plain,
        }

        if byte.is_some() {
            s.haystack = &s.haystack[1..];
        }
        byte = s.haystack.first().copied();
    }
}

impl<'a> Searcher<'a> {
    fn new(value: &'a Path) -> Result<Self, E> {
        let bytes = value.as_os_str().as_encoded_bytes();
        if bytes.len() <= 1 {
            match bytes.first() {
                Some(b'.') => Err(E::NotCanonical),
                it => Ok(Self {
                    haystack: &[],
                    level: 0,
                    err_slash: matches!(it, Some(b'/')),
                }),
            }
        } else {
            Ok(Self {
                haystack: bytes,
                level: 0,
                err_slash: matches!(bytes.first(), Some(b'/')),
            })
        }
    }
}

impl Iterator for Searcher<'_> {
    type Item = E;

    fn next(&mut self) -> Option<Self::Item> {
        search_next(self)
    }
}

fn check_namespace_prefix<'a>(
    path: &'a Path,
    name: usize,
    tail: &'a Path,
) -> (&'a Path, Option<E>) {
    let bytes = path.as_os_str().as_encoded_bytes();
    debug_assert!(matches!(bytes[0], b'/' | b'\\'));
    debug_assert!(matches!(bytes[1], b'/' | b'\\'));
    debug_assert!(matches!(bytes[2 + name], b'/' | b'\\'));
    debug_assert!(matches!(
        tail.as_os_str().as_encoded_bytes().first(),
        None | Some(b'/' | b'\\')
    ));

    let is_phony_root = len!(tail) == 1;
    if is_phony_root {
        (Path::new(""), Some(E::NotCanonical))
    } else if (bytes[0], bytes[1], bytes[2 + name]) != (b'\\', b'\\', b'\\') {
        (tail, Some(E::NotCanonical))
    } else {
        (tail, None)
    }
}

fn check_prefix(path: &Path) -> Option<(&Path, Option<E>)> {
    let (prefix, tail) = match prefix_of(path) {
        Some(pair) => pair,
        None => return Some((path, Some(E::NotAbsolute))),
    };

    use Prefix::*;
    match prefix.kind() {
        Disk(_) => match prefix.as_os_str().as_encoded_bytes()[0] {
            b'a'..=b'z' => Some((tail, Some(E::NotCanonical))),
            _ => Some((tail, None)),
        },
        DeviceNS(_) => Some(check_namespace_prefix(path, 1, tail)),
        UNC(server, _) => Some(check_namespace_prefix(path, server.len(), tail)),
        _ => None,
    }
}

fn check_path_quick(path: &Path) -> Result<(), E> {
    let tail = match check_prefix(path) {
        None => return Ok(()),
        Some((_, Some(err))) => return Err(err),
        Some((tail, None)) => tail,
    };

    match Searcher::new(tail)?.next() {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

fn check_path_parentless(path: &Path) -> Result<(), E> {
    let (tail, err) = match check_prefix(path) {
        None => return Ok(()),
        Some(pair) => pair,
    };

    let mut not_canonical = matches!(err, Some(E::NotCanonical));
    for err in Searcher::new(tail)? {
        match err {
            E::NotCanonical => not_canonical = true,
            E::ContainsParent => return Err(E::ContainsParent),
            E::NotAbsolute => unreachable!(),
        }
    }

    if not_canonical {
        Err(E::NotCanonical)
    } else {
        Ok(())
    }
}

fn check_path_canonical(path: &Path) -> Result<(), E> {
    let tail = match check_prefix(path) {
        None => return Ok(()),
        Some((_, Some(E::NotCanonical))) => return Err(E::NotCanonical),
        Some((tail, _)) => tail,
    };

    let mut has_parent = false;
    for err in Searcher::new(tail)? {
        match err {
            E::NotCanonical => return Err(E::NotCanonical),
            E::ContainsParent => has_parent = true,
            E::NotAbsolute => unreachable!(),
        }
    }

    if has_parent {
        Err(E::ContainsParent)
    } else {
        Ok(())
    }
}

fn normalize_drive_letter(path: &mut [u8]) {
    debug_assert!(matches!(path.first(), Some(b'a'..=b'z' | b'A'..=b'Z')));
    path[0].make_ascii_uppercase();
}

fn normalize_slash(path: &mut [u8], at: usize) {
    debug_assert!(matches!(path.get(at), Some(b'/' | b'\\')));
    path[at] = b'\\';
}

fn position_pop_slash(path: &[u8], start: usize, bottom: usize, pos: usize) -> Option<usize> {
    debug_assert!(start <= bottom);
    debug_assert!(bottom <= pos);
    debug_assert!(pos <= path.len());

    if bottom == pos {
        return None;
    }

    let begin = if bottom != start { bottom - 1 } else { start };
    let end = pos - 1;

    debug_assert!(begin == start || matches!(path.get(begin), Some(b'\\')));
    debug_assert!(matches!(path.get(end), None | Some(b'\\' | b'/')));

    let at = path[begin..end]
        .iter()
        .rposition(|&b| b == b'\\')
        .map_or(begin, |i| begin + i + 1);

    Some(at)
}

#[derive(Debug, Clone, Copy)]
enum PrefixCharacteristic {
    Disk,
    Namespace(usize, usize),
}

fn normalize_in_place(path: &mut Vec<u8>, prefix: Option<PrefixCharacteristic>) -> Option<E> {
    use PrefixCharacteristic::*;
    let start = match prefix {
        None => 0,
        Some(Disk) => {
            normalize_drive_letter(path);
            2
        }
        Some(Namespace(len, name)) => {
            normalize_slash(path, 0);
            normalize_slash(path, 1);
            normalize_slash(path, 2 + name);
            len
        }
    };

    enum Seen {
        Nothing,
        Slash,
        SlashDot,
        SlashDotDot,
    }
    use Seen::*;

    let mut pos = start;
    let (mut seen, mut bottom) = match path.get(start) {
        Some(b'/' | b'\\') => (Nothing, start + 1),
        _ => (Slash, start),
    };

    for i in start..path.len() {
        let byte = path[i];
        let push = match (seen, byte) {
            (Nothing, b'/' | b'\\') => {
                seen = Slash;
                Some(b'\\')
            }
            (Slash, b'/' | b'\\') => {
                seen = Slash;
                None
            }
            (Slash, b'.') => {
                seen = SlashDot;
                None
            }
            (SlashDot, b'/' | b'\\') => {
                seen = Slash;
                None
            }
            (SlashDot, b'.') => {
                seen = SlashDotDot;
                None
            }
            (SlashDot, byte) => {
                seen = Nothing;
                path[pos] = b'.'; // compensate
                pos += 1;
                Some(byte)
            }
            (SlashDotDot, b'/' | b'\\') => {
                seen = Slash;
                pos = match position_pop_slash(path, start, bottom, pos) {
                    Some(p) => p,
                    None => {
                        path[bottom] = b'.';
                        path[bottom + 1] = b'.';
                        path[bottom + 2] = b'\\';
                        bottom += 3;
                        bottom
                    }
                };

                None
            }
            (SlashDotDot, byte) => {
                seen = Nothing;
                path[pos] = b'.';
                path[pos + 1] = b'.';
                pos += 2;
                Some(byte)
            }
            (_, _) => {
                seen = Nothing;
                Some(byte)
            }
        };

        if let Some(byte) = push {
            path[pos] = byte;
            pos += 1;
        }
    }

    if matches!(seen, SlashDotDot) {
        if let Some(p) = position_pop_slash(path, start, bottom, pos) {
            pos = p;
            seen = Slash;
        } else {
            path[bottom] = b'.';
            path[bottom + 1] = b'.';
            bottom += 2;
            pos = bottom;
        }
    }

    if matches!(seen, Slash | SlashDot) {
        match prefix {
            Some(Disk) | None if pos > start + 1 => pos -= 1,
            Some(Namespace(_, _)) if pos > start => pos -= 1,
            _ => {}
        }
    }

    path.truncate(pos);

    if bottom >= start + 2 {
        Some(E::ContainsParent)
    } else {
        match prefix {
            None => Some(E::NotAbsolute),
            Some(Disk) if path.get(start) != Some(&b'\\') => Some(E::NotAbsolute),
            _ => None,
        }
    }
}

pub fn normalize(path: &mut PathBuf) -> Result<(), E> {
    use {Prefix::*, PrefixCharacteristic as C};
    let prefix = match prefix_of(path) {
        None => None,
        Some((it, _)) => match it.kind() {
            Disk(_) => Some(C::Disk),
            DeviceNS(_) => Some(C::Namespace(len!(it), 1)),
            UNC(server, _) => Some(C::Namespace(len!(it), server.len())),
            _ => return Ok(()),
        },
    };

    let mut bytes = mem::take(path).into_os_string().into_encoded_bytes();
    let error = normalize_in_place(&mut bytes, prefix);

    // SAFETY: normalization alters the sequence in three ways:
    // 1. it converts the ASCII drive letter to uppercase if it exists.
    // 2. it inserts a valid UTF-8 string at the boundary of `.`, `/` or `\`.
    // 3. it removes a subslice from the boundary of a `.`, `/` or `\` to the
    //    boundary of another `.`, `/` or `\`.
    //
    // Since `.`, `/` and `\` are all valid UTF-8 substrings, these three all
    // preserve the validity of the OS string encoding.
    *path = unsafe { OsString::from_encoded_bytes_unchecked(bytes) }.into();
    match error {
        None => Ok(()),
        Some(e) => Err(e),
    }
}

pub fn validate(path: &Path) -> Result<&Normpath, E> {
    if is_verbatim(path) {
        // SAFETY: verbatim paths are always valid
        return Ok(unsafe { cast_ref_unchecked(path) });
    }

    if !path.is_absolute() {
        return Err(E::NotAbsolute);
    }

    check_path_quick(path)?;

    // SAFETY: the path is already checked
    Ok(unsafe { cast_ref_unchecked(path) })
}

fn validate_fully(path: &Path) -> Result<&Normpath, E> {
    if is_verbatim(path) {
        // SAFETY: verbatim paths are always valid
        return Ok(unsafe { cast_ref_unchecked(path) });
    }

    if !path.is_absolute() {
        return Err(E::NotAbsolute);
    }

    check_path_parentless(path)?;

    // SAFETY: the path is already checked
    Ok(unsafe { cast_ref_unchecked(path) })
}

pub fn validate_canonical(path: &Path) -> Result<&Normpath, E> {
    if is_verbatim(path) {
        // SAFETY: verbatim paths are always valid
        return Ok(unsafe { cast_ref_unchecked(path) });
    }

    check_path_canonical(path)?;

    if !path.is_absolute() {
        return Err(E::NotAbsolute);
    }

    // SAFETY: the path is already checked
    Ok(unsafe { cast_ref_unchecked(path) })
}

pub fn validate_parentless(path: &Path) -> Result<&Normpath, E> {
    if is_verbatim(path) {
        // SAFETY: verbatim paths are always valid
        return Ok(unsafe { cast_ref_unchecked(path) });
    }

    check_path_parentless(path)?;

    if !path.is_absolute() {
        return Err(E::NotAbsolute);
    }

    // SAFETY: the path is already checked
    Ok(unsafe { cast_ref_unchecked(path) })
}

pub fn normalize_new_cow<'a>(path: &'a Path) -> Result<Cow<'a, Normpath>, E> {
    match validate_fully(path) {
        Ok(validated) => Ok(Cow::Borrowed(validated)),
        Err(E::NotCanonical) => {
            let mut path = path.into();
            normalize(&mut path).expect("should be able to normalize away non-canonicality");

            Ok(Cow::Owned(NormpathBuf(path)))
        }
        Err(e) => Err(e),
    }
}

pub fn normalize_new_buf<T>(path: T) -> Result<NormpathBuf, Ec<T>>
where
    T: AsRef<Path> + Into<PathBuf>,
{
    match validate_fully(path.as_ref()) {
        Ok(_) => Ok(NormpathBuf(path.into())),
        Err(E::NotCanonical) => {
            let mut path = path.into();
            normalize(&mut path).expect("should be able to normalize away non-canonicality");

            Ok(NormpathBuf(path))
        }
        Err(e) => Err(Ec::new(e, path)),
    }
}

fn compute_bottom_line(path: &Path, delta: &Path) -> Option<usize> {
    debug_assert!(path.is_absolute());
    assert_unixlike_relative(delta);

    let mut bottom = path;
    let mut growth = 0;
    for component in delta.components() {
        use Component::*;
        match component {
            CurDir => continue,
            Normal(_) => growth += 1,
            ParentDir if growth > 0 => growth -= 1,
            ParentDir => bottom = bottom.parent()?,
            _ => unreachable!(),
        }
    }

    Some(len!(bottom))
}

fn push_with_bottom_line(buf: &mut PathBuf, path: &Path) -> Result<(), E> {
    debug_assert!(buf.is_absolute());
    assert_unixlike_relative(path);

    let bottom = compute_bottom_line(buf, path).ok_or(E::ContainsParent)?;
    let mut bytes = mem::take(buf).into_os_string().into_encoded_bytes();
    // SAFETY: `bytes` is truncated to be the bottom line path, which is always
    // a valid OS string.
    bytes.truncate(bottom);
    *buf = unsafe { OsString::from_encoded_bytes_unchecked(bytes) }.into();

    for component in path.components() {
        use Component::*;
        match component {
            CurDir => continue,
            ParentDir if len!(buf) <= bottom => continue,
            Normal(name) => buf.push(name),
            ParentDir => assert!(buf.pop()),
            _ => unreachable!(),
        }
    }

    Ok(())
}

pub fn push(buf: &mut PathBuf, path: &Path) -> Result<(), E> {
    if is_verbatim(buf) {
        buf.push(path);
        return Ok(());
    }

    match check_path_parentless(path) {
        Ok(_) if path.has_root() => {
            buf.push(path);
        }
        Err(E::ContainsParent) if path.has_root() => {
            return Err(E::ContainsParent);
        }
        Err(E::NotCanonical) if path.has_root() => {
            buf.push(path);
            normalize(buf).expect("should be able to normalize away non-canonicality");
        }
        _ => {
            if path.is_relative() && !no_prefix(path) {
                return Err(E::NotAbsolute);
            } else {
                push_with_bottom_line(buf, path)?;
            }
        }
    }

    if is_phony_root(buf) {
        let mut bytes = mem::take(buf).into_os_string().into_encoded_bytes();
        bytes.pop();

        // SAFETY: the part removed is a valid UTF-8 substring (`\`), so the
        // remaining part is a valid OS string.
        *buf = unsafe { OsString::from_encoded_bytes_unchecked(bytes) }.into();
    }

    debug_assert!(validate(buf).is_ok());
    Ok(())
}

pub fn strip<'a>(path: &'a Path, base: &Path) -> Option<&'a Path> {
    let path = path.as_os_str().as_encoded_bytes();
    let base = base.as_os_str().as_encoded_bytes();

    if path.starts_with(base) {
        match path.get(base.len()) {
            None => Some(&[][..]),
            Some(b'\\') => Some(&path[base.len() + 1..]),
            _ => None,
        }
        // SAFETY: the slice starts immediately after a valid UTF-8 substring
        // (the separator), so it is also valid.
        .map(|bytes| unsafe { OsStr::from_encoded_bytes_unchecked(bytes) }.as_ref())
    } else {
        None
    }
}
