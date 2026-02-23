use std::{
    borrow::{Borrow, Cow},
    cmp,
    convert::Infallible,
    ffi::{OsStr, OsString},
    fmt::{self, Debug, Display},
    hash::{Hash, Hasher},
    ops::Deref,
    path::{Path, PathBuf},
    ptr,
    rc::Rc,
    sync::Arc,
};

/// The error type indicating why a path cannot be a valid [`Normpath`].
///
/// # Notes on Windows
///
/// On Windows, a parent directory component that can be normalized away (e.g.
/// `C:\foo\..`) is not considered as [`ContainsParent`], but instead as
/// [`NotCanonical`].
///
/// See [crate documentation](crate) for more details about that.
///
/// [`ContainsParent`]: Self::ContainsParent
/// [`NotCanonical`]: Self::NotCanonical
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Error {
    /// The path is not absolute.
    NotAbsolute,
    /// The path is not canonical.
    NotCanonical,
    /// The path contains a parent component that cannot be normalized away.
    ContainsParent,
}

impl From<Infallible> for Error {
    fn from(_: Infallible) -> Self {
        unreachable!()
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        const PARENT_ERR: &str = if cfg!(windows) {
            "path contains a '..' component that points outside of base directory"
        } else {
            "path contains a '..' component"
        };

        use Error::*;
        match self {
            NotAbsolute => f.write_str("path is not absolute"),
            NotCanonical => f.write_str("path is not canonical"),
            ContainsParent => f.write_str(PARENT_ERR),
        }
    }
}

impl std::error::Error for Error {}

/// The error type indicating why a path cannot be converted into a normalized
/// path.
///
/// This type is essentially an [`Error`] plus the original value.
#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub struct ConvertError<T> {
    /// The error indicating why the conversion failed.
    pub error: Error,
    /// The value that failed to convert.
    pub value: T,
}

impl<T> ConvertError<T> {
    /// Creates a new `ConvertError` with the given error and value.
    #[must_use]
    #[inline]
    pub const fn new(error: Error, value: T) -> Self {
        Self { error, value }
    }

    /// Maps the value by applying the given function, without changing the
    /// underlying error.
    #[inline]
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
    /// Clones the value, without changing the underlying error.
    #[inline]
    pub fn cloned(&self) -> ConvertError<T>
    where
        T: Clone,
    {
        ConvertError {
            error: self.error,
            value: self.value.clone(),
        }
    }

    /// Converts the value into an owned version, without changing the
    /// underlying error.
    #[inline]
    pub fn to_owned(&self) -> ConvertError<T::Owned>
    where
        T: ToOwned,
    {
        ConvertError {
            error: self.error,
            value: self.value.to_owned(),
        }
    }
}

impl<T> From<ConvertError<T>> for Error {
    /// Converts a [`ConvertError`] into an [`Error`] by discarding the value.
    #[inline]
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
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}: {:?}", self.error, self.value)
    }
}

impl<T: Debug> std::error::Error for ConvertError<T> {}

/// A slice of a normalized path (akin to [`Path`]).
///
/// Since every normalized path is a path, this type implements [`Deref`] to
/// [`Path`], and can be used wherever a `Path` is expected in most cases.
///
/// This type is `#[repr(transparent)]` over `Path`, as such, it is an *unsized*
/// type as well, which has to be used behind a pointer like `&` or [`Box`]. For
/// an owned version of this type, see [`NormpathBuf`].
///
/// Details about the normalization invariants can be found in the
/// [crate documentation](crate).
#[repr(transparent)]
pub struct Normpath(pub(crate) Path);

#[inline]
pub(crate) unsafe fn cast_ref_unchecked(path: &Path) -> &Normpath {
    // SAFETY: `#[repr(transparent)]` over Path.
    unsafe { &*(ptr::from_ref(path) as *const _) }
}

#[inline]
pub(crate) unsafe fn cast_box_unchecked(path: Box<Path>) -> Box<Normpath> {
    // SAFETY: `#[repr(transparent)]` over Path.
    unsafe { Box::from_raw(Box::into_raw(path) as *mut Normpath) }
}

#[inline]
unsafe fn cast_arc_unchecked(path: Arc<Path>) -> Arc<Normpath> {
    // SAFETY: `#[repr(transparent)]` over Path.
    unsafe { Arc::from_raw(Arc::into_raw(path) as *const Normpath) }
}

#[inline]
unsafe fn cast_rc_unchecked(path: Rc<Path>) -> Rc<Normpath> {
    // SAFETY: `#[repr(transparent)]` over Path.
    unsafe { Rc::from_raw(Rc::into_raw(path) as *const Normpath) }
}

#[inline]
fn backcast_box(path: Box<Normpath>) -> Box<Path> {
    // SAFETY: `#[repr(transparent)]` over Path.
    unsafe { Box::from_raw(Box::into_raw(path) as *mut Path) }
}

impl AsRef<Path> for Normpath {
    #[inline]
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<Normpath> for Normpath {
    #[inline]
    fn as_ref(&self) -> &Normpath {
        self
    }
}

impl AsRef<OsStr> for Normpath {
    #[inline]
    fn as_ref(&self) -> &OsStr {
        self.0.as_os_str()
    }
}

impl Borrow<Path> for Normpath {
    #[inline]
    fn borrow(&self) -> &Path {
        &self.0
    }
}

impl Deref for Normpath {
    type Target = Path;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Clone for Box<Normpath> {
    #[inline]
    fn clone(&self) -> Self {
        // SAFETY: `#[repr(transparent)]` over Path.
        let path = unsafe { &*(ptr::from_ref(self) as *const Box<Path>) };

        // SAFETY: the path is copied from a normalized path
        unsafe { cast_box_unchecked(path.clone()) }
    }
}

/// An owned normalized path that is mutable (akin to [`PathBuf`]).
///
/// This type provides methods like [`push`] that mutate the path in place,
/// while maintaining the normalization invariants. It also implements
/// [`Deref`] to [`Normpath`], meaning that all methods on [`Normpath`] slices
/// are available as well.
///
/// Details about the normalization invariants can be found in the
/// [crate documentation](crate).
///
/// [`push`]: Self::push
#[derive(Clone)]
pub struct NormpathBuf(pub(crate) PathBuf);

impl AsRef<Normpath> for NormpathBuf {
    #[inline]
    fn as_ref(&self) -> &Normpath {
        let p = self.0.as_path();

        // SAFETY: `#[repr(transparent)]` over Path.
        unsafe { cast_ref_unchecked(p) }
    }
}

impl AsRef<Path> for NormpathBuf {
    #[inline]
    fn as_ref(&self) -> &Path {
        &self.0
    }
}

impl AsRef<OsStr> for NormpathBuf {
    #[inline]
    fn as_ref(&self) -> &OsStr {
        self.0.as_os_str()
    }
}

impl Borrow<Normpath> for NormpathBuf {
    #[inline]
    fn borrow(&self) -> &Normpath {
        self.as_ref()
    }
}

impl Borrow<Path> for NormpathBuf {
    #[inline]
    fn borrow(&self) -> &Path {
        self.0.as_path()
    }
}

impl Deref for NormpathBuf {
    type Target = Normpath;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.as_ref()
    }
}

impl ToOwned for Normpath {
    type Owned = NormpathBuf;

    #[inline]
    fn to_owned(&self) -> Self::Owned {
        NormpathBuf(self.to_path_buf())
    }
}

impl From<&Normpath> for NormpathBuf {
    /// Creates a [`NormpathBuf`] from a [`Normpath`] slice.
    ///
    /// This will allocate and copy the data.
    #[inline]
    fn from(value: &Normpath) -> Self {
        Self(value.to_path_buf())
    }
}

impl From<&Normpath> for Box<Normpath> {
    /// Creates a boxed [`Normpath`] from a reference.
    ///
    /// This will allocate and copy the data.
    #[inline]
    fn from(value: &Normpath) -> Self {
        let value = Box::from(&value.0);
        // SAFETY: the path is copied from a normalized path
        unsafe { cast_box_unchecked(value) }
    }
}

impl<'a> From<&'a Normpath> for Cow<'a, Normpath> {
    /// Creates a clone-on-write pointer from a reference to [`Normpath`].
    ///
    /// This does not allocate or copy any data.
    #[inline]
    fn from(value: &'a Normpath) -> Self {
        Cow::Borrowed(value)
    }
}

impl From<&Normpath> for Arc<Normpath> {
    /// Creates an [`Arc`] pointer from a reference to [`Normpath`].
    ///
    /// This will allocate and copy the data.
    #[inline]
    fn from(value: &Normpath) -> Self {
        let value = Arc::<Path>::from(&value.0);
        // SAFETY: the path is copied from a normalized path
        unsafe { cast_arc_unchecked(value) }
    }
}

impl From<&Normpath> for Rc<Normpath> {
    /// Creates an [`Rc`] pointer from a reference to [`Normpath`].
    ///
    /// This will allocate and copy the data.
    #[inline]
    fn from(value: &Normpath) -> Self {
        let value = Rc::<Path>::from(&value.0);
        // SAFETY: the path is copied from a normalized path
        unsafe { cast_rc_unchecked(value) }
    }
}

impl From<Box<Normpath>> for NormpathBuf {
    /// Creates a [`NormpathBuf`] from a boxed [`Normpath`].
    ///
    /// This does not allocate or copy any data.
    #[inline]
    fn from(value: Box<Normpath>) -> Self {
        let value = backcast_box(value);
        Self(value.into_path_buf())
    }
}

impl From<Box<Normpath>> for PathBuf {
    /// Creates a [`PathBuf`] from a boxed [`Normpath`].
    ///
    /// This does not allocate or copy any data.
    #[inline]
    fn from(value: Box<Normpath>) -> Self {
        let value = backcast_box(value);
        value.into_path_buf()
    }
}

impl From<Box<Normpath>> for Box<Path> {
    /// Creates a boxed [`Path`] from a boxed [`Normpath`].
    ///
    /// This is a cost-free conversion.
    #[inline]
    fn from(value: Box<Normpath>) -> Self {
        backcast_box(value)
    }
}

impl From<NormpathBuf> for Box<Normpath> {
    /// Creates a boxed [`Normpath`] from a [`NormpathBuf`].
    ///
    /// This can allocate and copy the data depending on the implementation
    /// of the standard library, but typically it will not.
    #[inline]
    fn from(value: NormpathBuf) -> Self {
        let path = value.0.into_boxed_path();
        // SAFETY: the path is normalized per se
        unsafe { cast_box_unchecked(path) }
    }
}

impl From<NormpathBuf> for Cow<'_, Normpath> {
    /// Creates a clone-on-write pointer from a [`NormpathBuf`].
    ///
    /// This does not allocate or copy any data.
    #[inline]
    fn from(value: NormpathBuf) -> Self {
        Cow::Owned(value)
    }
}

impl From<NormpathBuf> for Arc<Normpath> {
    /// Creates an [`Arc`] pointer from a [`NormpathBuf`].
    ///
    /// This will allocate and copy the data.
    #[inline]
    fn from(value: NormpathBuf) -> Self {
        let path = Arc::<Path>::from(value.0);
        // SAFETY: the path is normalized per se
        unsafe { cast_arc_unchecked(path) }
    }
}

impl From<NormpathBuf> for Rc<Normpath> {
    /// Creates an [`Rc`] pointer from a [`NormpathBuf`].
    ///
    /// This will allocate and copy the data.
    #[inline]
    fn from(value: NormpathBuf) -> Self {
        let path = Rc::<Path>::from(value.0);
        // SAFETY: the path is normalized per se
        unsafe { cast_rc_unchecked(path) }
    }
}

impl From<NormpathBuf> for PathBuf {
    /// Creates a [`PathBuf`] from a [`NormpathBuf`].
    ///
    /// This is a cost-free conversion.
    #[inline]
    fn from(value: NormpathBuf) -> Self {
        value.0
    }
}

impl From<NormpathBuf> for OsString {
    /// Creates an [`OsString`] from a [`NormpathBuf`].
    ///
    /// This is a cost-free conversion.
    #[inline]
    fn from(value: NormpathBuf) -> Self {
        value.0.into_os_string()
    }
}

impl<'a> From<&'a NormpathBuf> for Cow<'a, Normpath> {
    /// Creates a clone-on-write pointer from a reference to [`NormpathBuf`].
    ///
    /// This does not allocate or copy any data.
    #[inline]
    fn from(value: &'a NormpathBuf) -> Self {
        Cow::Borrowed(value.as_ref())
    }
}

impl From<&NormpathBuf> for NormpathBuf {
    /// Clones a [`NormpathBuf`] from a reference to [`NormpathBuf`].
    #[inline]
    fn from(value: &NormpathBuf) -> Self {
        value.clone()
    }
}

impl From<Cow<'_, Normpath>> for NormpathBuf {
    /// Creates a [`NormpathBuf`] from a clone-on-write pointer to [`Normpath`].
    ///
    /// Converting from [`Cow::Owned`] does not allocate or copy any data.
    #[inline]
    fn from(value: Cow<'_, Normpath>) -> Self {
        use Cow::*;
        match value {
            Borrowed(value) => Self(value.to_path_buf()),
            Owned(value) => value,
        }
    }
}

impl<'a> From<Cow<'a, Normpath>> for Box<Normpath> {
    /// Creates a boxed [`Normpath`] from a clone-on-write pointer to
    /// [`Normpath`].
    ///
    /// Converting from [`Cow::Owned`] can allocate and copy the data depending
    /// on the implementation of the standard library, but typically it will
    /// not.
    #[inline]
    fn from(value: Cow<'a, Normpath>) -> Self {
        let path = match value {
            Cow::Borrowed(value) => Box::from(&value.0),
            Cow::Owned(value) => value.0.into_boxed_path(),
        };

        // SAFETY: the path is either copied from a normalized path, or
        // normalized per se
        unsafe { cast_box_unchecked(path) }
    }
}

impl PartialEq for Normpath {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0.as_os_str() == other.0.as_os_str()
    }
}

impl Eq for Normpath {}

impl Hash for Normpath {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_os_str().hash(state);
    }
}

impl Ord for Normpath {
    #[inline]
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.0.as_os_str().cmp(other.0.as_os_str())
    }
}

impl PartialOrd for Normpath {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq for NormpathBuf {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.0.as_os_str() == other.0.as_os_str()
    }
}

impl Eq for NormpathBuf {}

impl Hash for NormpathBuf {
    #[inline]
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.as_os_str().hash(state);
    }
}

impl Ord for NormpathBuf {
    #[inline]
    fn cmp(&self, other: &Self) -> cmp::Ordering {
        self.0.as_os_str().cmp(other.0.as_os_str())
    }
}

impl PartialOrd for NormpathBuf {
    #[inline]
    fn partial_cmp(&self, other: &Self) -> Option<cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl PartialEq<Normpath> for NormpathBuf {
    #[inline]
    fn eq(&self, other: &Normpath) -> bool {
        self.0.as_os_str() == other.0.as_os_str()
    }
}

impl PartialEq<NormpathBuf> for Normpath {
    #[inline]
    fn eq(&self, other: &NormpathBuf) -> bool {
        self.0.as_os_str() == other.0.as_os_str()
    }
}

impl PartialOrd<Normpath> for NormpathBuf {
    #[inline]
    fn partial_cmp(&self, other: &Normpath) -> Option<cmp::Ordering> {
        Some(self.0.as_os_str().cmp(other.0.as_os_str()))
    }
}

impl PartialOrd<NormpathBuf> for Normpath {
    #[inline]
    fn partial_cmp(&self, other: &NormpathBuf) -> Option<cmp::Ordering> {
        Some(self.0.as_os_str().cmp(other.0.as_os_str()))
    }
}

macro_rules! impl_cmp {
    (<$($life:lifetime),*> $lhs:ty, $rhs:ty) => {
        impl<$($life),*> PartialEq<$rhs> for $lhs {
            #[inline]
            fn eq(&self, other: &$rhs) -> bool {
                <Path as PartialEq>::eq(self.as_ref(), other.as_ref())
            }
        }

        impl<$($life),*> PartialEq<$lhs> for $rhs {
            #[inline]
            fn eq(&self, other: &$lhs) -> bool {
                <Path as PartialEq>::eq(other.as_ref(), self.as_ref())
            }
        }

        impl<$($life),*> PartialOrd<$rhs> for $lhs {
            #[inline]
            fn partial_cmp(&self, other: &$rhs) -> Option<std::cmp::Ordering> {
                <Path as PartialOrd>::partial_cmp(self.as_ref(), other.as_ref())
            }
        }

        impl<$($life),*> PartialOrd<$lhs> for $rhs {
            #[inline]
            fn partial_cmp(&self, other: &$lhs) -> Option<std::cmp::Ordering> {
                <Path as PartialOrd>::partial_cmp(other.as_ref(), self.as_ref())
            }
        }
    };
}

impl_cmp!(<> Normpath, Path);
impl_cmp!(<'a> Normpath, &'a Path);
impl_cmp!(<> Normpath, PathBuf);
impl_cmp!(<'a> Normpath, Cow<'a, Path>);
impl_cmp!(<> Normpath, OsStr);
impl_cmp!(<'a> Normpath, &'a OsStr);
impl_cmp!(<> Normpath, OsString);
impl_cmp!(<'a> Normpath, Cow<'a, OsStr>);

impl_cmp!(<> NormpathBuf, Path);
impl_cmp!(<'a> NormpathBuf, &'a Path);
impl_cmp!(<> NormpathBuf, PathBuf);
impl_cmp!(<'a> NormpathBuf, Cow<'a, Path>);
impl_cmp!(<> NormpathBuf, OsStr);
impl_cmp!(<'a> NormpathBuf, &'a OsStr);
impl_cmp!(<> NormpathBuf, OsString);
impl_cmp!(<'a> NormpathBuf, Cow<'a, OsStr>);

impl_cmp!(<'a> &'a Normpath, Path);
impl_cmp!(<'a> &'a Normpath, PathBuf);
impl_cmp!(<'a, 'b> &'a Normpath, Cow<'b, Path>);
impl_cmp!(<'a> &'a Normpath, OsStr);
impl_cmp!(<'a> &'a Normpath, OsString);
impl_cmp!(<'a, 'b> &'a Normpath, Cow<'b, OsStr>);

macro_rules! impl_eq_literal {
    ($lhs:ty, $rhs:ty) => {
        impl PartialEq<$rhs> for $lhs {
            #[inline]
            fn eq(&self, other: &$rhs) -> bool {
                <OsStr as PartialEq>::eq(self.as_ref(), other.as_ref())
            }
        }

        impl PartialEq<$lhs> for $rhs {
            #[inline]
            fn eq(&self, other: &$lhs) -> bool {
                <OsStr as PartialEq>::eq(other.as_ref(), self.as_ref())
            }
        }
    };
}

impl_eq_literal!(Normpath, str);
impl_eq_literal!(NormpathBuf, str);
impl_eq_literal!(Normpath, String);
impl_eq_literal!(NormpathBuf, String);

impl Debug for Normpath {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Debug::fmt(&self.0, f)
    }
}

impl Debug for NormpathBuf {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        Debug::fmt(&self.0, f)
    }
}
