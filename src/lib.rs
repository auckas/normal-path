use std::{
    borrow::{Borrow, Cow},
    cmp,
    convert::Infallible,
    ffi::{OsStr, OsString},
    fmt::{self, Debug, Display},
    hash::{Hash, Hasher},
    mem,
    ops::Deref,
    os::unix::ffi::{OsStrExt as _, OsStringExt as _},
    path::{Component, Path, PathBuf},
    ptr,
    str::FromStr,
};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum Error {
    #[error("path is not absolute")]
    NotAbsolute,
    #[error("path is not minimal")]
    NotMinimal,
    #[error("path contains a '..' component")]
    ContainsParent,
}
use Error as E;

impl From<Infallible> for Error {
    fn from(_: Infallible) -> Self {
        unreachable!()
    }
}

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub struct ConvertError<T> {
    pub error: Error,
    pub value: T,
}
use ConvertError as Ec;

impl<T> ConvertError<T> {
    #[must_use]
    pub const fn new(error: Error, value: T) -> Self {
        Self { error, value }
    }

    #[must_use]
    pub fn map<F, U>(self, f: F) -> ConvertError<U>
    where
        F: FnOnce(T) -> U,
    {
        ConvertError {
            error: self.error,
            value: f(self.value),
        }
    }
}

impl<T: ?Sized> ConvertError<&T> {
    #[must_use]
    pub fn cloned(&self) -> ConvertError<T>
    where
        T: Clone,
    {
        ConvertError {
            error: self.error.clone(),
            value: self.value.clone(),
        }
    }

    #[must_use]
    pub fn to_owned(&self) -> ConvertError<T::Owned>
    where
        T: ToOwned,
    {
        ConvertError {
            error: self.error.clone(),
            value: self.value.to_owned(),
        }
    }
}

impl<T> From<ConvertError<T>> for Error {
    fn from(value: ConvertError<T>) -> Self {
        value.error
    }
}

impl<T> From<Infallible> for ConvertError<T> {
    fn from(_: Infallible) -> Self {
        unreachable!()
    }
}

impl<T: Debug> Display for ConvertError<T> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {:?}", self.error, self.value)
    }
}

#[repr(transparent)]
pub struct Normpath(Path);

unsafe fn ref_unchecked(path: &Path) -> &Normpath {
    // SAFETY: Normpath is `#[repr(transparent)]` over Path.
    unsafe { &*(ptr::from_ref(path) as *const _) }
}

unsafe fn boxed_unchecked(path: Box<Path>) -> Box<Normpath> {
    // SAFETY: same as `ref_unchecked`.
    unsafe { mem::transmute::<Box<Path>, Box<Normpath>>(path) }
}

fn reduce_box(path: Box<Normpath>) -> Box<Path> {
    // SAFETY: same as `ref_unchecked`.
    unsafe { mem::transmute::<Box<Normpath>, Box<Path>>(path) }
}

impl AsRef<Path> for Normpath {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Normpath> for Normpath {
    fn as_ref(&self) -> &Normpath {
        self
    }
}

impl AsRef<OsStr> for Normpath {
    fn as_ref(&self) -> &OsStr {
        self.0.as_os_str()
    }
}

impl Borrow<Path> for Normpath {
    fn borrow(&self) -> &Path {
        &self.0
    }
}

impl Deref for Normpath {
    type Target = Path;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Clone for Box<Normpath> {
    fn clone(&self) -> Self {
        // SAFETY: same as `ref_unchecked`.
        let path = unsafe { &*(ptr::from_ref(self) as *const Box<Path>) };

        // SAFETY: the path is a Normpath per se
        unsafe { boxed_unchecked(path.clone()) }
    }
}

#[derive(Clone)]
pub struct NormpathBuf(PathBuf);

impl AsRef<Normpath> for NormpathBuf {
    fn as_ref(&self) -> &Normpath {
        let p = self.0.as_path();
        unsafe { ref_unchecked(p) }
    }
}

impl AsRef<Path> for NormpathBuf {
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<OsStr> for NormpathBuf {
    fn as_ref(&self) -> &OsStr {
        self.0.as_os_str()
    }
}

impl Borrow<Normpath> for NormpathBuf {
    fn borrow(&self) -> &Normpath {
        self.as_ref()
    }
}

impl Borrow<Path> for NormpathBuf {
    fn borrow(&self) -> &Path {
        self.0.as_path()
    }
}

impl Deref for NormpathBuf {
    type Target = Normpath;

    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl ToOwned for Normpath {
    type Owned = NormpathBuf;

    fn to_owned(&self) -> Self::Owned {
        NormpathBuf(self.to_path_buf())
    }
}

struct Searcher<'a> {
    haystack: &'a [u8],
}

fn search_next(s: &mut Searcher<'_>) -> Option<Error> {
    use Error as E;

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
                return Some(E::NotMinimal);
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

impl<'a> From<&'a [u8]> for Searcher<'a> {
    fn from(value: &'a [u8]) -> Self {
        if value.len() > 1 {
            Self { haystack: value }
        } else {
            Self { haystack: &[] }
        }
    }
}

impl Iterator for Searcher<'_> {
    type Item = Error;

    fn next(&mut self) -> Option<Self::Item> {
        search_next(self)
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

    let mut seen = match path.starts_with(b"/") {
        true => Nothing,
        false => Slash,
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

    if has_parent {
        Some(E::ContainsParent)
    } else if !path.starts_with(b"/") {
        Some(E::NotAbsolute)
    } else {
        None
    }
}

fn check_component_short(path: &Path) -> Result<(), E> {
    let raw = path.as_os_str().as_bytes();
    debug_assert!(raw.len() <= 2);

    if raw.is_empty() {
        Ok(())
    } else if raw.len() == 1 {
        match raw[0] {
            b'.' => Err(E::NotMinimal),
            b'/' => Ok(()),
            _ => Ok(()),
        }
    } else {
        match (raw[0], raw[1]) {
            (b'.', b'.') => Err(E::ContainsParent),
            (b'/', b'.') | (_, b'/') => Err(E::NotMinimal),
            _ => Ok(()),
        }
    }
}

fn check_component_quick(path: &Path) -> Result<(), E> {
    if path.as_os_str().len() <= 2 {
        return check_component_short(path);
    }

    match Searcher::from(path.as_os_str().as_bytes()).next() {
        Some(err) => Err(err),
        None => Ok(()),
    }
}

fn check_component_fully(path: &Path) -> Result<(), E> {
    if path.as_os_str().len() <= 2 {
        return check_component_short(path);
    }

    let mut needs_normalization = false;
    for err in Searcher::from(path.as_os_str().as_bytes()) {
        use Error::*;
        match err {
            NotMinimal => needs_normalization = true,
            ContainsParent => return Err(E::ContainsParent),
            NotAbsolute => unreachable!(),
        }
    }

    match needs_normalization {
        true => Err(E::NotMinimal),
        false => Ok(()),
    }
}

fn check_component_normal(path: &Path) -> Result<(), E> {
    if path.as_os_str().len() <= 2 {
        return check_component_short(path);
    }

    let mut has_parent = false;
    for err in Searcher::from(path.as_os_str().as_bytes()) {
        use Error::*;
        match err {
            NotMinimal => return Err(E::NotMinimal),
            ContainsParent => has_parent = true,
            NotAbsolute => unreachable!(),
        }
    }

    match has_parent {
        true => Err(E::ContainsParent),
        false => Ok(()),
    }
}

fn validate(path: &Path) -> Result<&Normpath, E> {
    use Error as E;

    if !path.is_absolute() || path.as_os_str().is_empty() {
        return Err(E::NotAbsolute);
    }

    check_component_quick(path)?;

    // SAFETY: the path is already checked
    Ok(unsafe { ref_unchecked(path) })
}

fn validate_fully(path: &Path) -> Result<&Normpath, E> {
    if !path.is_absolute() || path.as_os_str().is_empty() {
        return Err(E::NotAbsolute);
    }

    check_component_fully(path)?;

    // SAFETY: the path is already checked
    Ok(unsafe { ref_unchecked(path) })
}

fn validate_minimal(path: &Path) -> Result<&Normpath, E> {
    check_component_normal(path)?;

    if !path.is_absolute() || path.as_os_str().is_empty() {
        return Err(E::NotAbsolute);
    }

    // SAFETY: the path is already checked
    Ok(unsafe { ref_unchecked(path) })
}

fn validate_parentless(path: &Path) -> Result<&Normpath, E> {
    check_component_fully(path)?;

    if !path.is_absolute() || path.as_os_str().is_empty() {
        return Err(E::NotAbsolute);
    }

    // SAFETY: the path is already checked
    Ok(unsafe { ref_unchecked(path) })
}

fn validate_cow<'a>(path: &'a Path) -> Result<Cow<'a, Normpath>, E> {
    match validate_fully(path) {
        Ok(p) => Ok(Cow::Borrowed(p)),
        Err(E::NotMinimal) => Ok(Cow::Owned(NormpathBuf(path.components().collect()))),
        Err(e) => Err(e),
    }
}

fn normalize<T>(path: T) -> Result<NormpathBuf, Ec<T>>
where
    T: AsRef<Path> + Into<PathBuf>,
{
    use Error::*;
    match validate_fully(path.as_ref()) {
        Ok(_) => Ok(NormpathBuf(path.into())),
        Err(NotMinimal) => {
            let mut bytes = path.into().into_os_string().into_vec();
            let error = normalize_in_place(&mut bytes);
            debug_assert_eq!(error, None);

            let path = PathBuf::from(OsString::from_vec(bytes));
            Ok(NormpathBuf(path))
        }
        Err(e) => Err(Ec::new(e, path)),
    }
}

impl From<&Normpath> for NormpathBuf {
    fn from(value: &Normpath) -> Self {
        Self(value.to_path_buf())
    }
}

impl From<&Normpath> for Box<Normpath> {
    fn from(value: &Normpath) -> Self {
        let value = Box::from(&value.0);
        // SAFETY: the path is copied from a Normpath
        unsafe { boxed_unchecked(value) }
    }
}

impl<'a> From<&'a Normpath> for Cow<'a, Normpath> {
    fn from(value: &'a Normpath) -> Self {
        Cow::Borrowed(value)
    }
}

impl From<Box<Normpath>> for NormpathBuf {
    fn from(value: Box<Normpath>) -> Self {
        let value = reduce_box(value);
        Self(value.into_path_buf())
    }
}

impl From<Box<Normpath>> for PathBuf {
    fn from(value: Box<Normpath>) -> Self {
        let value = reduce_box(value);
        value.into_path_buf()
    }
}

impl From<Box<Normpath>> for Box<Path> {
    fn from(value: Box<Normpath>) -> Self {
        reduce_box(value)
    }
}

impl From<Cow<'_, Normpath>> for NormpathBuf {
    fn from(value: Cow<'_, Normpath>) -> Self {
        use Cow::*;
        match value {
            Borrowed(value) => Self(value.to_path_buf()),
            Owned(value) => value,
        }
    }
}

impl<'a> From<Cow<'a, Normpath>> for Box<Normpath> {
    fn from(value: Cow<'a, Normpath>) -> Self {
        let path = match value {
            Cow::Borrowed(value) => Box::from(&value.0),
            Cow::Owned(value) => value.0.into_boxed_path(),
        };

        // SAFETY: the path is either copied from a Normpath, or a Normpath per se
        unsafe { boxed_unchecked(path) }
    }
}

impl<'a> From<&'a NormpathBuf> for Cow<'a, Normpath> {
    fn from(value: &'a NormpathBuf) -> Self {
        Cow::Borrowed(value.as_ref())
    }
}

impl From<NormpathBuf> for Box<Normpath> {
    fn from(value: NormpathBuf) -> Self {
        let path = value.0.into_boxed_path();
        // SAFETY: the path is a Normpath per se
        unsafe { boxed_unchecked(path) }
    }
}

impl From<NormpathBuf> for Cow<'_, Normpath> {
    fn from(value: NormpathBuf) -> Self {
        Cow::Owned(value)
    }
}

impl From<NormpathBuf> for PathBuf {
    fn from(value: NormpathBuf) -> Self {
        value.0
    }
}

impl From<NormpathBuf> for OsString {
    fn from(value: NormpathBuf) -> Self {
        value.0.into_os_string()
    }
}

impl From<&NormpathBuf> for NormpathBuf {
    fn from(value: &NormpathBuf) -> Self {
        value.clone()
    }
}

macro_rules! impl_try_from {
    (owned $t:ty) => {
        impl TryFrom<$t> for NormpathBuf {
            type Error = ConvertError<$t>;

            fn try_from(value: $t) -> Result<Self, Self::Error> {
                normalize(value)
            }
        }
    };
    (&$life:lifetime $t:ty) => {
        impl<$life> TryFrom<&$life $t> for NormpathBuf {
            type Error = ConvertError<&$life $t>;

            fn try_from(value: &$life $t) -> Result<Self, Self::Error> {
                normalize(PathBuf::from(value))
                    .map_err(|e| Ec::new(e.error, value))
            }
        }
    };
}

impl_try_from!(owned String);
impl_try_from!(owned OsString);
impl_try_from!(owned PathBuf);
impl_try_from!(owned Box<Path>);

impl_try_from!(&'a str);
impl_try_from!(&'a String);
impl_try_from!(&'a OsStr);
impl_try_from!(&'a OsString);
impl_try_from!(&'a Path);
impl_try_from!(&'a PathBuf);

impl PartialEq for Normpath {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_os_str() == other.0.as_os_str()
    }
}

impl Eq for Normpath {}

impl Hash for Normpath {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_os_str().hash(state);
    }
}

impl Ord for Normpath {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.0.as_os_str().cmp(other.0.as_os_str())
    }
}

impl PartialOrd for Normpath {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for NormpathBuf {
    fn eq(&self, other: &Self) -> bool {
        self.0.as_os_str() == other.0.as_os_str()
    }
}

impl Eq for NormpathBuf {}

impl Hash for NormpathBuf {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_os_str().hash(state);
    }
}

impl Ord for NormpathBuf {
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.0.as_os_str().cmp(other.0.as_os_str())
    }
}

impl PartialOrd for NormpathBuf {
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq<Normpath> for NormpathBuf {
    fn eq(&self, other: &Normpath) -> bool {
        self.0.as_os_str() == other.0.as_os_str()
    }
}

impl PartialEq<NormpathBuf> for Normpath {
    fn eq(&self, other: &NormpathBuf) -> bool {
        self.0.as_os_str() == other.0.as_os_str()
    }
}

impl PartialOrd<Normpath> for NormpathBuf {
    fn partial_cmp(&self, other: &Normpath) -> Option<cmp::Ordering> {
        Some(self.0.as_os_str().cmp(other.0.as_os_str()))
    }
}

impl PartialOrd<NormpathBuf> for Normpath {
    fn partial_cmp(&self, other: &NormpathBuf) -> Option<cmp::Ordering> {
        Some(self.0.as_os_str().cmp(other.0.as_os_str()))
    }
}

macro_rules! impl_cmp {
    (<$($life:lifetime),*> $lhs:ty, $rhs:ty) => {
        impl<$($life),*> PartialEq<$rhs> for $lhs {
            fn eq(&self, other: &$rhs) -> bool {
                <Path as PartialEq>::eq(self.as_ref(), other.as_ref())
            }
        }

        impl<$($life),*> PartialEq<$lhs> for $rhs {
            fn eq(&self, other: &$lhs) -> bool {
                <Path as PartialEq>::eq(other.as_ref(), self.as_ref())
            }
        }

        impl<$($life),*> PartialOrd<$rhs> for $lhs {
            fn partial_cmp(&self, other: &$rhs) -> Option<std::cmp::Ordering> {
                <Path as PartialOrd>::partial_cmp(self.as_ref(), other.as_ref())
            }
        }

        impl<$($life),*> PartialOrd<$lhs> for $rhs {
            fn partial_cmp(&self, other: &$lhs) -> Option<std::cmp::Ordering> {
                <Path as PartialOrd>::partial_cmp(other.as_ref(), self.as_ref())
            }
        }
    };
}

impl_cmp!(<> Normpath, Path);
impl_cmp!(<> Normpath, PathBuf);
impl_cmp!(<> Normpath, OsStr);
impl_cmp!(<> Normpath, OsString);
impl_cmp!(<'a> Normpath, Cow<'a, OsStr>);
impl_cmp!(<> NormpathBuf, Path);
impl_cmp!(<> NormpathBuf, PathBuf);
impl_cmp!(<> NormpathBuf, OsStr);
impl_cmp!(<> NormpathBuf, OsString);
impl_cmp!(<'a> NormpathBuf, Cow<'a, OsStr>);

fn push(buf: &mut PathBuf, path: &Path) -> Result<(), Error> {
    let needs_normalization = match check_component_fully(path) {
        Ok(_) => false,
        Err(E::NotMinimal) => true,
        Err(e) => return Err(e),
    };

    if !needs_normalization {
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

impl Normpath {
    #[must_use]
    pub fn root() -> &'static Normpath {
        // SAFETY: "/" is a normalized path
        unsafe { ref_unchecked(Path::new("/")) }
    }

    pub fn validate<S>(path: &S) -> Result<&Normpath, Error>
    where
        S: AsRef<OsStr> + ?Sized,
    {
        validate(Path::new(path))
    }

    pub fn validate_minimal<S>(path: &S) -> Result<&Normpath, Error>
    where
        S: AsRef<OsStr> + ?Sized,
    {
        validate_minimal(Path::new(path))
    }

    pub fn validate_parentless<S>(path: &S) -> Result<&Normpath, Error>
    where
        S: AsRef<OsStr> + ?Sized,
    {
        validate_parentless(Path::new(path))
    }

    pub fn normalize<'a, S>(path: &'a S) -> Result<Cow<'a, Normpath>, Error>
    where
        S: AsRef<OsStr> + ?Sized,
    {
        validate_cow(Path::new(path))
    }

    /// # Safety
    ///
    /// The caller must ensure that `path` is a normalized path, such that
    /// `validate` would succeed.
    #[must_use]
    pub unsafe fn new_unchecked<S>(path: &S) -> &Normpath
    where
        S: AsRef<OsStr> + ?Sized,
    {
        let path = Path::new(path.as_ref());
        unsafe { ref_unchecked(path) }
    }

    #[must_use]
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.0.as_os_str().len()
    }

    #[must_use]
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    #[must_use]
    pub fn parent(&self) -> Option<&Normpath> {
        let parent = self.0.parent()?;
        // SAFETY: the parent of a normalized path is also normalized
        Some(unsafe { ref_unchecked(parent) })
    }

    pub fn join<P: AsRef<Path>>(&self, path: P) -> Result<NormpathBuf, Error> {
        let mut buf = self.0.to_path_buf();
        push(&mut buf, path.as_ref())?;

        Ok(NormpathBuf(buf))
    }
}

impl NormpathBuf {
    #[must_use]
    pub fn root() -> Self {
        Self(PathBuf::from("/"))
    }

    pub fn validate<P>(path: P) -> Result<Self, ConvertError<P>>
    where
        P: AsRef<OsStr> + Into<OsString>,
    {
        match validate(Path::new(&path)) {
            Ok(_) => Ok(Self(PathBuf::from(path.into()))),
            Err(e) => Err(Ec::new(e, path)),
        }
    }

    pub fn normalize(path: PathBuf) -> Result<Self, ConvertError<PathBuf>> {
        let mut bytes = path.into_os_string().into_vec();
        let error = normalize_in_place(&mut bytes);

        let path = PathBuf::from(OsString::from_vec(bytes));
        match error {
            None => Ok(Self(path)),
            Some(e) => Err(Ec::new(e, path)),
        }
    }

    pub fn push<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Error> {
        push(&mut self.0, path.as_ref())
    }

    pub fn pop(&mut self) -> Result<(), Error> {
        if self.0.as_path().as_os_str().len() == 1 {
            return Err(E::NotAbsolute);
        }

        let popped = self.0.pop();
        debug_assert!(popped);

        Ok(())
    }

    pub fn into_boxed_path(self) -> Box<Normpath> {
        let value = self.0.into_boxed_path();
        // SAFETY: the path is a Normpath per se
        unsafe { boxed_unchecked(value) }
    }

    #[must_use]
    pub fn into_os_string(self) -> OsString {
        self.0.into_os_string()
    }
}

impl Debug for Normpath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Debug::fmt(self.as_path(), f)
    }
}

impl Debug for NormpathBuf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

#[cfg(feature = "serde")]
impl Serialize for Normpath {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_path().serialize(serializer)
    }
}

#[cfg(feature = "serde")]
impl Serialize for NormpathBuf {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_path().serialize(serializer)
    }
}

impl FromStr for NormpathBuf {
    type Err = ConvertError<String>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        normalize(s.to_string())
    }
}

impl FromStr for Box<Normpath> {
    type Err = ConvertError<String>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        normalize(s.to_string()).map(|p| p.into_boxed_path())
    }
}

#[cfg(feature = "serde")]
impl<'a, 'de: 'a> Deserialize<'de> for &'a Normpath {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let path = <&Path>::deserialize(deserializer)?;
        validate(path).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for NormpathBuf {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let path = PathBuf::deserialize(deserializer)?;
        normalize(path).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "serde")]
impl<'de> Deserialize<'de> for Box<Normpath> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let path = PathBuf::deserialize(deserializer)?;
        normalize(path)
            .map(|p| p.into_boxed_path())
            .map_err(serde::de::Error::custom)
    }
}

mod testing;

#[cfg(test)]
mod tests {
    use std::{iter, path::Component};

    use fastrand::Rng;

    use super::*;

    fn is_minimal(path: &Path) -> bool {
        let normalized = path
            .components()
            .filter(|c| !matches!(c, Component::CurDir))
            .collect::<PathBuf>();
        path.as_os_str() == normalized.as_os_str()
    }

    fn is_parentless(path: &Path) -> bool {
        path.components()
            .all(|c| !matches!(c, Component::ParentDir))
    }

    fn is_reproducible(path: &Path) -> bool {
        path.is_absolute() && is_minimal(path) && is_parentless(path)
    }

    fn into_source<T>(error: Ec<T>) -> E {
        error.error
    }

    #[test]
    pub fn root() {
        Normpath::validate("/").unwrap();
        NormpathBuf::try_from("/").unwrap();
    }

    #[test]
    pub fn empty() {
        assert_eq!(Normpath::validate(""), Err(E::NotAbsolute));
        assert_eq!(
            NormpathBuf::try_from("").map_err(into_source),
            Err(E::NotAbsolute),
        );
    }

    #[test]
    pub fn normalize() {
        let mut rng = Rng::new();
        let paths = iter::from_fn(move || Some(testing::draw_any(rng.usize(1..64))));

        for path in paths.take(1024) {
            let reference = Path::new(&path)
                .components()
                .filter(|c| !matches!(c, Component::CurDir))
                .collect::<PathBuf>();

            let ours = NormpathBuf::normalize(PathBuf::from(&path))
                .map(PathBuf::from)
                .unwrap_or_else(|e| e.value);

            assert_eq!(
                reference.as_os_str(),
                ours.as_os_str(),
                "normalization differs: {path:?}",
            );
        }
    }

    fn test_push(subject: &mut NormpathBuf, reference: &mut PathBuf, component: &str) {
        if is_parentless(component.as_ref()) {
            reference.push(component);
            subject.push(component).unwrap();

            assert_eq!(subject.as_path(), reference.as_path());
        } else {
            let error = subject.push(component).unwrap_err();
            assert_eq!(error, E::ContainsParent);
        }
    }

    #[test]
    pub fn push_relative() {
        for _ in 0..128 {
            let mut rng = Rng::new();
            let components = iter::from_fn(move || Some(testing::draw_rel(rng.usize(1..16))));

            let mut path = NormpathBuf::root();
            let mut reference = PathBuf::from("/");
            for component in components.take(64) {
                test_push(&mut path, &mut reference, &component);
            }
        }
    }

    #[test]
    pub fn push_absolute() {
        for _ in 0..128 {
            let mut rng = Rng::new();
            let components = iter::from_fn(move || Some(testing::draw_abs(rng.usize(1..32))));

            let mut path = NormpathBuf::root();
            let mut reference = PathBuf::from("/");
            for component in components.take(32) {
                test_push(&mut path, &mut reference, &component);
            }
        }
    }

    #[test]
    pub fn regular() {
        let mut rng = Rng::new();
        let paths = iter::from_fn(move || Some(testing::draw_re(rng.usize(1..64))));

        for path in paths.take(256) {
            let ref_validated = Normpath::validate(&path).unwrap();
            assert!(is_reproducible(ref_validated.as_path()));
            assert_eq!(ref_validated, Path::new(&path));

            let ref_v_normal = Normpath::validate_minimal(&path).unwrap();
            assert_eq!(ref_v_normal, ref_validated);

            let ref_v_parentless = Normpath::validate_parentless(&path).unwrap();
            assert_eq!(ref_v_parentless, ref_validated);

            let buf_validated = NormpathBuf::validate(&path).unwrap();
            assert_eq!(&buf_validated, ref_validated);

            let buf_normalized = NormpathBuf::normalize(PathBuf::from(&path)).unwrap();
            assert_eq!(&buf_normalized, ref_validated);

            let buf_converted = NormpathBuf::try_from(path.clone()).unwrap();
            assert_eq!(&buf_converted, ref_validated);
        }
    }

    fn test_single_err(path: &str, error: E) {
        assert_eq!(Normpath::validate(&path), Err(error.clone()));
        assert_eq!(Normpath::validate_minimal(&path), Err(error.clone()),);
        assert_eq!(Normpath::validate_parentless(&path), Err(error.clone()),);

        assert_eq!(
            NormpathBuf::validate(&path).map_err(into_source),
            Err(error.clone()),
        );
        assert_eq!(
            NormpathBuf::normalize(PathBuf::from(&path)).map_err(into_source),
            Err(error.clone()),
        );

        let conv_err = NormpathBuf::try_from(path).unwrap_err();
        assert_eq!(conv_err.error, error);
        assert_eq!(conv_err.value, path);
    }

    #[test]
    pub fn err_single_relative() {
        let mut rng = Rng::new();
        let paths = iter::from_fn(move || Some(testing::draw_rel(rng.usize(1..64))))
            .filter(|p| is_parentless(p.as_ref()) && is_minimal(p.as_ref()));

        for path in paths.take(256) {
            test_single_err(&path, E::NotAbsolute);
        }
    }

    #[test]
    pub fn err_single_parent() {
        let mut rng = Rng::new();
        let paths = iter::from_fn(move || Some(testing::draw_abs(rng.usize(1..64))))
            .filter(|p| !is_parentless(Path::new(p)) && is_minimal(Path::new(p)));

        for path in paths.take(256) {
            test_single_err(&path, E::ContainsParent);
        }
    }

    #[test]
    pub fn err_single_verbose() {
        let mut rng = Rng::new();
        let paths = iter::from_fn(move || Some(testing::draw_abs(rng.usize(1..64))))
            .filter(|p| !is_minimal(Path::new(p)) && is_parentless(Path::new(p)));

        for path in paths.take(256) {
            assert_eq!(Normpath::validate(&path), Err(E::NotMinimal));
            assert_eq!(Normpath::validate_minimal(&path), Err(E::NotMinimal));
            assert_eq!(Normpath::validate_parentless(&path), Err(E::NotMinimal));

            assert_eq!(
                NormpathBuf::validate(&path).map_err(into_source),
                Err(E::NotMinimal),
            );

            let ref_normalized = Normpath::normalize(&path).unwrap();
            assert!(is_reproducible(&ref_normalized));
            assert_eq!(&*ref_normalized, Path::new(&path));

            let buf_normalized = NormpathBuf::normalize(PathBuf::from(&path)).unwrap();
            assert_eq!(&buf_normalized, &*ref_normalized);

            let buf_converted = NormpathBuf::try_from(&path).unwrap();
            assert_eq!(&buf_converted, &buf_normalized);
        }
    }

    #[test]
    pub fn err_preference() {
        let mut rng = Rng::new();
        let paths = iter::from_fn(move || Some(testing::draw_abs(rng.usize(1..64))))
            .filter(|p| !is_minimal(Path::new(p)) && !is_parentless(Path::new(p)));

        for path in paths.take(256) {
            assert_eq!(Normpath::validate_parentless(&path), Err(E::ContainsParent));
            assert_eq!(Normpath::validate_minimal(&path), Err(E::NotMinimal));
        }
    }
}
