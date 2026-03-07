#![cfg(unix)]

use std::{
    borrow::Cow,
    ffi::{OsStr, OsString},
    mem,
    os::unix::ffi::{OsStrExt as _, OsStringExt as _},
    path::{Component, Path, PathBuf},
};

use super::super::trivial::{
    cast_ref_unchecked, ConvertError as Ec, Error as E, Normpath, NormpathBuf,
};

macro_rules! bytes {
    ($path:expr) => {
        $path.as_os_str().as_bytes()
    };
}

#[derive(Debug)]
struct Searcher<'a> {
    haystack: &'a [u8],
}

fn search_next(s: &mut Searcher<'_>) -> Option<E> {
    enum Seen {
        Nothing,
        Slash,
        SlashDot,
        SlashDotDot,
    }
    use Seen::*;

    let mut byte = s.haystack.first().copied();
    let mut seen = match byte {
        None | Some(b'/') => Nothing,
        _ => Slash,
    };

    loop {
        match (seen, byte) {
            (Slash | SlashDot, None | Some(b'/')) => {
                return Some(E::NotCanonical);
            }
            (SlashDotDot, None | Some(b'/')) => {
                return Some(E::ContainsParent);
            }
            (Nothing, None) => {
                return None;
            }
            (Nothing, Some(b'/')) => seen = Slash,
            (Slash, Some(b'.')) => seen = SlashDot,
            (SlashDot, Some(b'.')) => seen = SlashDotDot,
            (_, Some(_)) => seen = Nothing,
        }

        if byte.is_some() {
            s.haystack = &s.haystack[1..];
        }
        byte = s.haystack.first().copied();
    }
}

impl<'a> Searcher<'a> {
    fn new(value: &'a Path) -> Result<Self, E> {
        let bytes = bytes!(value);
        if bytes.is_empty() {
            Err(E::NotCanonical)
        } else if bytes.len() <= 1 {
            Ok(Self { haystack: &[] })
        } else {
            Ok(Self { haystack: bytes })
        }
    }
}

impl Iterator for Searcher<'_> {
    type Item = E;

    fn next(&mut self) -> Option<Self::Item> {
        search_next(self)
    }
}

fn check_component_quick(path: &Path) -> Result<(), E> {
    match Searcher::new(path)?.next() {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

fn check_component_parentless(path: &Path) -> Result<(), E> {
    let mut not_canonical = false;
    for err in Searcher::new(path)? {
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

fn check_component_canonical(path: &Path) -> Result<(), E> {
    let mut has_parent = false;
    for err in Searcher::new(path)? {
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

fn normalize_in_place(path: &mut Vec<u8>) -> Option<E> {
    let mut pos = 0;
    let mut has_parent = false;

    enum Seen {
        Nothing,
        Slash,
        SlashDot,
        SlashDotDot,
    }
    use Seen::*;

    let mut seen = match path.first() {
        Some(b'/') => Nothing,
        _ => Slash,
    };

    for i in 0..path.len() {
        let byte = path[i];
        let drop = match (seen, byte) {
            (Nothing, b'/') => {
                seen = Slash;
                false
            }
            (Slash, b'/') => {
                seen = Slash;
                true
            }
            (Slash, b'.') => {
                seen = SlashDot;
                true
            }
            (SlashDot, b'/') => {
                seen = Slash;
                true
            }
            (SlashDot, byte) => {
                seen = if byte == b'.' { SlashDotDot } else { Nothing };

                path[pos] = b'.'; // compensate
                pos += 1;
                false
            }
            (SlashDotDot, b'/') => {
                seen = Slash;
                has_parent = true;
                false
            }
            (_, _) => {
                seen = Nothing;
                false
            }
        };

        if !drop {
            path[pos] = byte;
            pos += 1;
        }
    }

    match seen {
        SlashDot | Slash if pos > 1 => pos -= 1,
        SlashDotDot => has_parent = true,
        _ => {}
    }

    path.truncate(pos);
    if pos == 0 {
        path.push(b'.');
    }

    if has_parent {
        Some(E::ContainsParent)
    } else if !path.starts_with(b"/") {
        Some(E::NotAbsolute)
    } else {
        None
    }
}

pub fn normalize(path: &mut PathBuf) -> Result<(), E> {
    let mut bytes = mem::take(path).into_os_string().into_vec();
    let error = normalize_in_place(&mut bytes);

    *path = PathBuf::from(OsString::from_vec(bytes));
    match error {
        None => Ok(()),
        Some(e) => Err(e),
    }
}

pub fn validate(path: &Path) -> Result<&Normpath, E> {
    if !path.is_absolute() {
        return Err(E::NotAbsolute);
    }

    check_component_quick(path)?;

    // SAFETY: the path is already checked
    Ok(unsafe { cast_ref_unchecked(path) })
}

fn validate_fully(path: &Path) -> Result<&Normpath, E> {
    if !path.is_absolute() {
        return Err(E::NotAbsolute);
    }

    check_component_parentless(path)?;

    // SAFETY: the path is already checked
    Ok(unsafe { cast_ref_unchecked(path) })
}

pub fn validate_canonical(path: &Path) -> Result<&Normpath, E> {
    check_component_canonical(path)?;

    if !path.is_absolute() {
        return Err(E::NotAbsolute);
    }

    // SAFETY: the path is already checked
    Ok(unsafe { cast_ref_unchecked(path) })
}

pub fn validate_parentless(path: &Path) -> Result<&Normpath, E> {
    check_component_parentless(path)?;

    if !path.is_absolute() {
        return Err(E::NotAbsolute);
    }

    // SAFETY: the path is already checked
    Ok(unsafe { cast_ref_unchecked(path) })
}

pub fn normalize_new_cow<'a>(path: &'a Path) -> Result<Cow<'a, Normpath>, E> {
    match validate_fully(path) {
        Ok(p) => Ok(Cow::Borrowed(p)),
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

pub fn push(buf: &mut PathBuf, path: &Path) -> Result<(), E> {
    let canonical = match check_component_parentless(path) {
        Ok(_) => bytes!(path) != b".",
        Err(E::NotCanonical) => false,
        Err(e) => return Err(e),
    };

    if canonical {
        buf.push(path);
    } else {
        for component in path.components() {
            use Component::*;
            match component {
                RootDir => buf.push("/"),
                CurDir => {}
                Normal(name) => buf.push(name),
                _ => unreachable!(),
            }
        }
    }

    debug_assert!(validate(buf).is_ok());
    Ok(())
}

pub fn strip<'a>(path: &'a Path, base: &Path) -> Option<&'a Path> {
    let path = bytes!(path);
    let base = bytes!(base);

    if path.starts_with(base) {
        match path.get(base.len()) {
            None => Some(&[][..]),
            Some(b'/') => Some(&path[base.len() + 1..]),
            _ => None,
        }
        .map(|bytes| Path::new(OsStr::from_bytes(bytes)))
    } else {
        None
    }
}
