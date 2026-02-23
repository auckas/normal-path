use std::{
    borrow::Cow,
    path::{Path, PathBuf},
};

use super::trivial::{ConvertError as Ec, Error as E, Normpath, NormpathBuf};

mod unix;
mod windows;

mod this {
    #[cfg(unix)]
    pub use super::unix::*;
    #[cfg(windows)]
    pub use super::windows::*;
}

#[inline(always)]
pub fn normalize(path: &mut PathBuf) -> Result<(), E> {
    this::normalize(path)
}

#[inline(always)]
pub fn validate(path: &Path) -> Result<&Normpath, E> {
    this::validate(path)
}

#[inline(always)]
pub fn validate_canonical(path: &Path) -> Result<&Normpath, E> {
    this::validate_canonical(path)
}

#[inline(always)]
pub fn validate_parentless(path: &Path) -> Result<&Normpath, E> {
    this::validate_parentless(path)
}

#[inline(always)]
pub fn normalize_new_cow<'a>(path: &'a Path) -> Result<Cow<'a, Normpath>, E> {
    this::normalize_new_cow(path)
}

#[inline(always)]
pub fn normalize_new_buf<T>(path: T) -> Result<NormpathBuf, Ec<T>>
where
    T: AsRef<Path> + Into<PathBuf>,
{
    this::normalize_new_buf(path)
}

#[inline(always)]
pub fn push(buf: &mut PathBuf, path: &Path) -> Result<(), E> {
    this::push(buf, path)
}

#[inline(always)]
pub fn strip<'a>(path: &'a Path, base: &Path) -> Option<&'a Path> {
    this::strip(path, base)
}
