use std::{
    borrow::Cow,
    collections::TryReserveError,
    ffi::{OsStr, OsString},
    path::{Components, Path, PathBuf, PrefixComponent},
    str::FromStr,
};

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

use super::{
    imp,
    trivial::{cast_box_unchecked, cast_ref_unchecked, ConvertError, Error, Normpath, NormpathBuf},
};

macro_rules! delegate {
    ($platform:ident => $expr:expr) => {{
        #[cfg($platform)]
        {
            Some($expr)
        }
        #[cfg(not($platform))]
        {
            None
        }
    }};
}

impl Normpath {
    /// Wraps the root path `/` as a `Normpath` slice.
    ///
    /// This is a cost-free conversion.
    #[cfg(any(unix, docsrs))]
    #[cfg_attr(docsrs, doc(cfg(unix)))]
    #[must_use]
    #[inline]
    pub fn unix_root() -> &'static Self {
        // SAFETY: "/" is a normalized path
        unsafe { cast_ref_unchecked(Path::new("/")) }
    }

    /// Wraps the root path as a `Normpath` slice, if there is one for the local
    /// platform.
    ///
    /// This is equivalent to [`unix_root`] on Unix, while returning [`None`] on
    /// other platforms.
    ///
    /// [`unix_root`]: Self::unix_root
    #[must_use]
    #[inline]
    pub fn root() -> Option<&'static Self> {
        delegate!(unix => Self::unix_root())
    }

    /// Validates that `path` is normalized to wrap it as a `Normpath` slice.
    ///
    /// Among all possible [`Error`] variants, this function always fails fast
    /// with the first one encountered. See [`validate_canonical`] and
    /// [`validate_parentless`] if certain variants are of particular interest.
    ///
    /// # Errors
    ///
    /// If `path` is not normalized, returns an [`Error`].
    ///
    /// # Examples
    ///
    /// ```rust,ignore-windows
    /// use normal_path::{Normpath, Error};
    ///
    /// let norm = "/foo/bar";
    /// let path1 = "foo/bar";
    /// let path2 = "/foo/bar/";
    /// let path3 = "/foo/../bar";
    ///
    /// assert!(Normpath::validate(norm).is_ok());
    /// assert_eq!(Normpath::validate(path1), Err(Error::NotAbsolute));
    /// assert_eq!(Normpath::validate(path2), Err(Error::NotCanonical));
    /// assert_eq!(Normpath::validate(path3), Err(Error::ContainsParent));
    /// ```
    ///
    /// [`validate_canonical`]: Self::validate_canonical
    /// [`validate_parentless`]: Self::validate_parentless
    #[inline]
    pub fn validate<S: AsRef<OsStr> + ?Sized>(path: &S) -> Result<&Self, Error> {
        imp::validate(Path::new(path))
    }

    /// Validates that `path` is normalized to wrap it as a `Normpath` slice,
    /// with a focus on the canonicality of the path.
    ///
    /// As such, the function may search the entire path for non-canonical
    /// patterns even with the presence of other errors. See [`validate`] if a
    /// fast failure is preferred.
    ///
    /// # Errors
    ///
    /// If `path` is not normalized, returns an [`Error`] with a certain order
    /// of precedence among all possible variants:
    /// 1. [`Error::NotCanonical`]
    /// 2. [`Error::ContainsParent`]
    /// 3. [`Error::NotAbsolute`]
    ///
    /// # Notes on Windows
    ///
    /// On Windows, a parent directory component that can be normalized
    /// lexically (e.g. `C:\foo\..`) is considered as [`Error::NotCanonical`]
    /// instead of [`Error::ContainsParent`].
    ///
    /// See [crate documentation](crate) for more details about that.
    ///
    /// # Examples
    ///
    /// ```rust,ignore-windows
    /// use normal_path::{Normpath, Error};
    ///
    /// let norm = "/foo/bar";
    /// assert!(Normpath::validate_canonical(norm).is_ok());
    ///
    /// let path1 = "/foo/../bar/.";
    /// let path2 = "/foo/../bar";
    /// assert_eq!(Normpath::validate_canonical(path1), Err(Error::NotCanonical));
    /// assert_eq!(Normpath::validate_canonical(path2), Err(Error::ContainsParent));
    /// ```
    ///
    /// [`validate`]: Self::validate
    #[inline]
    pub fn validate_canonical<S: AsRef<OsStr> + ?Sized>(path: &S) -> Result<&Self, Error> {
        imp::validate_canonical(Path::new(path))
    }

    /// Validates that `path` is normalized to wrap it as a `Normpath` slice,
    /// with a focus on whether the path contains parent components that
    /// cannot be normalized away.
    ///
    /// As such, the function may search the entire path for parent components
    /// even with the presence of other errors. See [`validate`] if a fast
    /// failure is preferred.
    ///
    /// # Errors
    ///
    /// If `path` is not normalized, returns an [`Error`] with a certain order
    /// of precedence among all possible variants:
    /// 1. [`Error::ContainsParent`]
    /// 2. [`Error::NotCanonical`]
    /// 3. [`Error::NotAbsolute`]
    ///
    /// # Notes on Windows
    ///
    /// On Windows, a parent directory component that can be normalized
    /// lexically (e.g. `C:\foo\..`) is considered as [`Error::NotCanonical`]
    /// instead of [`Error::ContainsParent`].
    ///
    /// See [crate documentation](crate) for more details about that.
    ///
    /// # Examples
    ///
    /// ```rust,ignore-windows
    /// use normal_path::{Normpath, Error};
    ///
    /// let norm = "/foo/bar";
    /// assert!(Normpath::validate_parentless(norm).is_ok());
    ///
    /// let path1 = "/foo/./bar/..";
    /// let path2 = "/foo/./bar";
    /// assert_eq!(Normpath::validate_parentless(path1), Err(Error::ContainsParent));
    /// assert_eq!(Normpath::validate_parentless(path2), Err(Error::NotCanonical));
    /// ```
    ///
    /// Windows deals with parent components differently:
    ///
    /// ```rust
    /// # #[cfg(windows)] {
    /// use normal_path::{Normpath, Error};
    ///
    /// let path1 = r"C:\foo\.\bar\..";
    /// let path2 = r"C:\foo\.\bar";
    /// assert_eq!(Normpath::validate_parentless(path1), Err(Error::NotCanonical));
    /// assert_eq!(Normpath::validate_parentless(path2), Err(Error::NotCanonical));
    ///
    /// let path3 = r"C:\foo\.\bar\..\..\..";
    /// assert_eq!(Normpath::validate_parentless(path3), Err(Error::ContainsParent));
    /// # }
    /// ```
    ///
    /// [`validate`]: Self::validate
    #[inline]
    pub fn validate_parentless<S: AsRef<OsStr> + ?Sized>(path: &S) -> Result<&Self, Error> {
        imp::validate_parentless(Path::new(path))
    }

    /// Validates that `path` is normalized to wrap it as a `Normpath` slice
    /// while trying to normalize non-canonical patterns.
    ///
    /// - If the path is already normalized, a borrowed slice is returned.
    ///
    /// - If the path only contains non-canonical patterns, an owned version is
    ///   returned as the result of normalization.
    ///
    /// # Errors
    ///
    /// If `path` is not absolute or contains parent components that cannot be
    /// normalized away, returns an [`Error`]. This implies [`NotCanonical`]
    /// will never be returned.
    ///
    /// [`NotCanonical`]: Error::NotCanonical
    ///
    /// # Examples
    ///
    /// ```rust,ignore-windows
    /// use std::borrow::Cow;
    /// use normal_path::{Normpath, Error};
    ///
    /// let norm = Normpath::normalize("/foo/bar").unwrap();
    /// let path1 = Normpath::normalize("foo/bar").unwrap_err();
    /// let path2 = Normpath::normalize("/foo/./bar/").unwrap();
    /// let path3 = Normpath::normalize("/foo/../bar").unwrap_err();
    ///
    /// assert!(matches!(norm, Cow::Borrowed(_)));
    /// assert_eq!(&*norm, "/foo/bar");
    ///
    /// assert_eq!(path1, Error::NotAbsolute);
    ///
    /// assert!(matches!(path2, Cow::Owned(_)));
    /// assert_eq!(&*path2, "/foo/bar");
    ///
    /// assert_eq!(path3, Error::ContainsParent);
    /// ```
    ///
    /// Windows deals with parent components differently:
    ///
    /// ```rust
    /// # #[cfg(windows)] {
    /// use std::borrow::Cow;
    /// use normal_path::{Normpath, Error};
    ///
    /// let path3 = Normpath::normalize(r"C:\foo\..\bar").unwrap();
    /// let path4 = Normpath::normalize(r"C:\foo\..\bar\..\..").unwrap_err();
    ///
    /// assert!(matches!(path3, Cow::Owned(_)));
    /// assert_eq!(&*path3, r"C:\bar");
    ///
    /// assert_eq!(path4, Error::ContainsParent);
    /// # }
    /// ```
    #[inline]
    pub fn normalize<S: AsRef<OsStr> + ?Sized>(path: &S) -> Result<Cow<'_, Self>, Error> {
        imp::normalize_new_cow(Path::new(path))
    }

    /// Wraps `path` as a `Normpath` slice without any validation.
    ///
    /// # Safety
    ///
    /// `path` must be a normalized path, such that [`validate`] would succeed.
    ///
    /// # Examples
    ///
    /// ```rust,ignore-windows
    /// use normal_path::Normpath;
    ///
    /// let path = "/foo/bar";
    /// assert!(Normpath::validate(path).is_ok());
    ///
    /// // SAFETY: already validated
    /// let norm = unsafe { Normpath::new_unchecked(path) };
    /// ```
    ///
    /// [`validate`]: Self::validate
    #[must_use]
    #[inline]
    pub unsafe fn new_unchecked<S: AsRef<OsStr> + ?Sized>(path: &S) -> &Self {
        let path = Path::new(path.as_ref());
        unsafe { cast_ref_unchecked(path) }
    }

    /// Returns the length of `self` in bytes.
    #[must_use]
    #[inline]
    #[allow(clippy::len_without_is_empty)]
    pub fn len(&self) -> usize {
        self.0.as_os_str().len()
    }

    /// Yields the underlying [`Path`] slice.
    #[must_use]
    #[inline]
    pub fn as_path(&self) -> &Path {
        &self.0
    }

    /// Returns the `Normpath` without the final component, if the path doesn't
    /// end with the root.
    ///
    /// This function effectively shadows [`Path::parent`] but is entirely
    /// compatible in most cases. In case the original method is desired,
    /// use `self.as_path().parent()` to invoke it.
    #[must_use]
    #[inline]
    pub fn parent(&self) -> Option<&Self> {
        let parent = self.0.parent()?;
        // SAFETY: the parent of a normalized path is also normalized
        Some(unsafe { cast_ref_unchecked(parent) })
    }

    /// Splits [`self.components`] into the prefix part and the rest.
    ///
    /// [`self.components`]: Path::components
    ///
    /// # Examples
    ///
    /// ```rust
    /// # #[cfg(windows)] {
    /// use std::path::{Component, Prefix};
    /// use normal_path::Normpath;
    ///
    /// let path = Normpath::validate(r"C:\a").unwrap();
    /// let (prefix, mut components) = path.windows_split_components();
    ///
    /// assert_eq!(prefix.kind(), Prefix::Disk(b'C'));
    /// assert_eq!(components.next(), Some(Component::RootDir));
    /// assert_eq!(components.next(), Some(Component::Normal("a".as_ref())));
    /// assert_eq!(components.next(), None);
    /// # }
    /// ```
    #[cfg(any(windows, docsrs))]
    #[cfg_attr(docsrs, doc(cfg(windows)))]
    pub fn windows_split_components(&self) -> (PrefixComponent<'_>, Components<'_>) {
        use std::path::Component::Prefix;

        let mut components = self.0.components();
        let Some(Prefix(prefix)) = components.next() else {
            unreachable!()
        };

        (prefix, components)
    }

    /// Returns the prefix component of `self`.
    #[cfg(any(windows, docsrs))]
    #[cfg_attr(docsrs, doc(cfg(windows)))]
    #[must_use]
    #[inline]
    pub fn windows_prefix(&self) -> PrefixComponent<'_> {
        self.windows_split_components().0
    }

    /// Splits [`self.components`] into the prefix part and the rest, if there
    /// is a prefix.
    ///
    /// This is equivalent to [`windows_split_components`] on Windows, while
    /// returning [`None`] on other platforms.
    ///
    /// [`self.components`]: Path::components
    /// [`windows_split_components`]: Self::windows_split_components
    ///
    /// # Examples
    ///
    /// ```rust
    /// use std::path::Component;
    /// use normal_path::Normpath;
    ///
    /// let path = if cfg!(windows) { r"C:\a" } else { "/a" };
    /// let norm = Normpath::validate(path).unwrap();
    ///
    /// let mut components = match norm.split_components() {
    ///     Some((_, components)) => components,
    ///     None => norm.components(),
    /// };
    ///
    /// assert_eq!(components.next(), Some(Component::RootDir));
    /// assert_eq!(components.next(), Some(Component::Normal("a".as_ref())));
    /// assert_eq!(components.next(), None);
    /// ```
    #[must_use]
    #[inline]
    pub fn split_components(&self) -> Option<(PrefixComponent<'_>, Components<'_>)> {
        delegate!(windows => self.windows_split_components())
    }

    /// Returns the prefix component of `self`, if there is one.
    ///
    /// This is equivalent to [`windows_prefix`] on Windows, while returning
    /// [`None`] on other platforms.
    ///
    /// [`windows_prefix`]: Self::windows_prefix
    #[must_use]
    #[inline]
    pub fn prefix(&self) -> Option<PrefixComponent<'_>> {
        delegate!(windows => self.windows_prefix())
    }

    /// Creates an owned [`NormpathBuf`] with `path` adjoined to `self`, with
    /// normalization.
    ///
    /// See [`PathBuf::push`] for more details on what it means to adjoin a
    /// path.
    ///
    /// # Errors
    ///
    /// If the resulting path cannot be normalized, returns an [`Error`]. This
    /// implies [`NotCanonical`] will never be returned.
    ///
    /// [`NotCanonical`]: Error::NotCanonical
    #[inline]
    pub fn checked_join<P: AsRef<Path>>(&self, path: P) -> Result<NormpathBuf, Error> {
        let mut buf = self.0.to_path_buf();
        imp::push(&mut buf, path.as_ref())?;

        Ok(NormpathBuf(buf))
    }

    /// Returns a path that, when joined onto `base`, yields `self`.
    ///
    /// If `base` is not a prefix of `self`, returns [`None`].
    ///
    /// Compared to [`Path::strip_prefix`], this function only performs
    /// *byte-level* comparison, skipping the parsing as an optimization.
    ///
    /// # Examples
    ///
    /// ```rust,ignore-windows
    /// use std::path::Path;
    /// use normal_path::Normpath;
    ///
    /// let norm = Normpath::validate("/foo/bar/baz").unwrap();
    /// let base1 = Path::new("/foo/bar");
    /// let base2 = Path::new("/foo/./bar");
    /// let base3 = Path::new("/foo/ba");
    ///
    /// // strip_prefix parses paths and thus both are accepted
    /// assert!(norm.strip_prefix(base1).is_ok());
    /// assert!(norm.strip_prefix(base2).is_ok());
    ///
    /// // But quick_strip_prefix only compares bytes
    /// assert!(norm.quick_strip_prefix(base1).is_some());
    /// assert!(norm.quick_strip_prefix(base2).is_none());
    ///
    /// // Both functions are otherwise the same
    /// assert!(norm.strip_prefix(base3).is_err());
    /// assert!(norm.quick_strip_prefix(base3).is_none());
    /// ```
    #[must_use]
    #[inline]
    pub fn quick_strip_prefix<P: AsRef<Path>>(&self, base: P) -> Option<&Path> {
        imp::strip(&self.0, base.as_ref())
    }

    /// Determines whether `base` is a prefix of `self`.
    ///
    /// Compared to [`Path::starts_with`], this function only performs
    /// *byte-level* comparison, skipping the parsing as an optimization.
    ///
    /// # Examples
    ///
    /// ```rust,ignore-windows
    /// use std::path::Path;
    /// use normal_path::Normpath;
    ///
    /// let norm = Normpath::validate("/foo/bar/baz").unwrap();
    /// let base1 = Path::new("/foo/bar");
    /// let base2 = Path::new("/foo/./bar");
    /// let base3 = Path::new("/foo/ba");
    ///
    /// // starts_with parses paths and thus both are accepted
    /// assert!(norm.starts_with(base1));
    /// assert!(norm.starts_with(base2));
    ///
    /// // But quick_starts_with only compares bytes
    /// assert!(norm.quick_starts_with(base1));
    /// assert!(!norm.quick_starts_with(base2));
    ///
    /// // Both functions are otherwise the same
    /// assert!(!norm.starts_with(base3));
    /// assert!(!norm.quick_starts_with(base3));
    /// ```
    #[must_use]
    #[inline]
    pub fn quick_starts_with<P: AsRef<Path>>(&self, base: P) -> bool {
        imp::strip(&self.0, base.as_ref()).is_some()
    }
}

impl NormpathBuf {
    /// Creates a new `NormpathBuf` from the root path `/`.
    #[cfg(unix)]
    #[cfg_attr(docsrs, doc(cfg(unix)))]
    #[must_use]
    #[inline]
    pub fn root() -> Self {
        Self(PathBuf::from("/"))
    }

    /// Validates that `path` is normalized to create a `NormpathBuf` from it.
    ///
    /// This function is an owned version of [`Normpath::validate`]. Refer to it
    /// for more details.
    ///
    /// # Errors
    ///
    /// If `path` is not normalized, returns an [`ConvertError`] containing the
    /// original `path` instance unchanged.
    ///
    /// # Examples
    ///
    /// ```rust,ignore-windows
    /// use normal_path::{NormpathBuf, Error};
    ///
    /// let norm = "/foo/bar";
    /// let path1 = "foo/bar";
    /// let path2 = "/foo/bar/";
    /// let path3 = "/foo/../bar";
    ///
    /// assert!(NormpathBuf::validate(norm).is_ok());
    ///
    /// let e1 = NormpathBuf::validate(path1).unwrap_err();
    /// assert_eq!((e1.error, e1.value), (Error::NotAbsolute, path1));
    /// let e2 = NormpathBuf::validate(path2).unwrap_err();
    /// assert_eq!((e2.error, e2.value), (Error::NotCanonical, path2));
    /// let e3 = NormpathBuf::validate(path3).unwrap_err();
    /// assert_eq!((e3.error, e3.value), (Error::ContainsParent, path3));
    /// ```
    #[inline]
    pub fn validate<P>(path: P) -> Result<Self, ConvertError<P>>
    where
        P: AsRef<OsStr> + Into<OsString>,
    {
        match imp::validate(Path::new(&path)) {
            Ok(_) => Ok(Self(PathBuf::from(path.into()))),
            Err(e) => Err(ConvertError::new(e, path)),
        }
    }

    /// Validates that `path` is normalized to create a `NormpathBuf` from it
    /// while trying to normalize non-canonical patterns.
    ///
    /// This function will *mutate* the input `path` to normalize *every*
    /// non-canonical pattern found, even with the presence of other errors.
    ///
    /// # Caveats
    ///
    /// This function mutates the input `path` *regardless* of the result. Use
    /// [`Normpath::normalize`] or the corresponding [`TryFrom`] implementation
    /// if mutation is not desired.
    ///
    /// # Errors
    ///
    /// If `path` is not absolute or contains parent components that cannot be
    /// normalized away, returns an [`ConvertError`] containing the `path`,
    /// with all non-canonicality removed. This implies [`NotCanonical`] will
    /// never be returned.
    ///
    /// # Examples
    ///
    /// ```rust,ignore-windows
    /// use std::path::PathBuf;
    /// use normal_path::{Error, NormpathBuf};
    ///
    /// let path1 = PathBuf::from("//foo/./bar/");
    /// let norm1 = NormpathBuf::normalize(path1).unwrap();
    /// assert_eq!(&norm1, "/foo/bar");
    ///
    /// let path2 = PathBuf::from(".//foo/../bar/");
    /// let err2 = NormpathBuf::normalize(path2).unwrap_err();
    /// assert_eq!(&err2.value, "foo/../bar");
    /// ```
    ///
    /// The results are different on Windows:
    ///
    /// ```rust
    /// # #[cfg(windows)] {
    /// use std::path::PathBuf;
    /// use normal_path::{Error, NormpathBuf};
    ///
    /// let path1 = PathBuf::from(r"C:\foo/.\bar\");
    /// let norm1 = NormpathBuf::normalize(path1).unwrap();
    /// assert_eq!(&norm1, r"C:\foo\bar");
    ///
    /// let path2 = PathBuf::from(r"./\foo\../bar\..");
    /// let err2 = NormpathBuf::normalize(path2).unwrap_err();
    /// assert_eq!(&err2.value, "");
    /// # }
    /// ```
    ///
    /// [`NotCanonical`]: Error::NotCanonical
    #[inline]
    pub fn normalize(mut path: PathBuf) -> Result<Self, ConvertError<PathBuf>> {
        match imp::normalize(&mut path) {
            Ok(_) => Ok(Self(path)),
            Err(e) => Err(ConvertError::new(e, path)),
        }
    }

    /// Creates a `NormpathBuf` from `path` without any validation.
    ///
    /// # Safety
    ///
    /// `path` must be a normalized path, such that [`validate`] would succeed.
    ///
    /// # Examples
    ///
    /// ```rust,ignore-windows
    /// use normal_path::NormpathBuf;
    ///
    /// let path = "/foo/bar";
    /// assert!(NormpathBuf::validate(path).is_ok());
    ///
    /// // SAFETY: already validated
    /// let norm = unsafe { NormpathBuf::new_unchecked(path) };
    /// ```
    ///
    /// [`validate`]: Self::validate
    #[must_use]
    #[inline]
    pub unsafe fn new_unchecked<S: Into<OsString>>(path: S) -> Self {
        Self(PathBuf::from(path.into()))
    }

    /// Extends `self` with `path`, with normalization.
    ///
    /// See [`PathBuf::push`] for more details on how `self` is extended with
    /// `path`.
    ///
    /// Consider using [`Normpath::checked_join`] if you need a new
    /// `NormpathBuf` instead of using this function on a cloned `NormpathBuf`.
    ///
    /// # Errors
    ///
    /// If the resulting path cannot be normalized, returns an [`Error`]. This
    /// implies [`NotCanonical`] will never be returned.
    ///
    /// [`NotCanonical`]: Error::NotCanonical
    #[inline]
    pub fn push<P: AsRef<Path>>(&mut self, path: P) -> Result<(), Error> {
        imp::push(&mut self.0, path.as_ref())
    }

    /// Truncates `self` to [`self.parent`].
    ///
    /// Returns `false` and does nothing if [`self.parent`] is [`None`].
    ///
    /// [`self.parent`]: Normpath::parent
    #[inline]
    pub fn pop(&mut self) -> bool {
        self.0.pop()
    }

    /// Coerces to a [`Normpath`] slice.
    #[must_use]
    #[inline]
    pub fn as_normpath(&self) -> &Normpath {
        self.as_ref()
    }

    /// Converts the `NormpathBuf` into a [boxed](Box) [`Normpath`].
    #[must_use]
    #[inline]
    pub fn into_boxed_path(self) -> Box<Normpath> {
        let value = self.0.into_boxed_path();
        // SAFETY: the path is a Normpath per se
        unsafe { cast_box_unchecked(value) }
    }

    /// Consumes the `NormpathBuf`, yielding the underlying [`PathBuf`]
    /// instance.
    #[must_use]
    #[inline]
    pub fn into_path_buf(self) -> PathBuf {
        self.0
    }

    /// Consumes the `NormpathBuf`, yielding its internal [`OsString`] storage.
    #[must_use]
    #[inline]
    pub fn into_os_string(self) -> OsString {
        self.0.into_os_string()
    }

    /// Invokes [`capacity`](OsString::capacity) on the internal [`OsString`]
    /// storage.
    #[inline]
    pub fn capacity(&self) -> usize {
        self.0.capacity()
    }

    /// Invokes [`reserve`](OsString::reserve) on the internal [`OsString`]
    /// storage.
    #[inline]
    pub fn reserve(&mut self, additional: usize) {
        self.0.reserve(additional)
    }

    /// Invokes [`reserve_exact`](OsString::reserve_exact) on the internal
    /// [`OsString`] storage.
    #[inline]
    pub fn reserve_exact(&mut self, additional: usize) {
        self.0.reserve_exact(additional)
    }

    /// Invokes [`try_reserve`](OsString::try_reserve) on the internal
    /// [`OsString`] storage.
    #[inline]
    pub fn try_reserve(&mut self, additional: usize) -> Result<(), TryReserveError> {
        self.0.try_reserve(additional)
    }

    /// Invokes [`try_reserve_exact`](OsString::try_reserve_exact) on the
    /// internal [`OsString`] storage.
    #[inline]
    pub fn try_reserve_exact(&mut self, additional: usize) -> Result<(), TryReserveError> {
        self.0.try_reserve_exact(additional)
    }

    /// Invokes [`shrink_to_fit`](OsString::shrink_to_fit) on the internal
    /// [`OsString`] storage.
    #[inline]
    pub fn shrink_to_fit(&mut self) {
        self.0.shrink_to_fit()
    }

    /// Invokes [`shrink_to`](OsString::shrink_to) on the internal [`OsString`]
    /// storage.
    #[inline]
    pub fn shrink_to(&mut self, min_capacity: usize) {
        self.0.shrink_to(min_capacity)
    }
}

macro_rules! impl_try_from {
    (owned over <$($life:lifetime)?> $t:ty) => {
        impl<$($life)?> TryFrom<$t> for NormpathBuf {
            type Error = ConvertError<$t>;

            /// Validates that `value` is normalized to create a [`NormpathBuf`]
            /// from it.
            ///
            /// Unlike [`NormpathBuf::normalize`], this function never mutates
            /// the input `value`.
            fn try_from(value: $t) -> Result<Self, Self::Error> {
                imp::normalize_new_buf(value)
            }
        }
    };
    (&$life:lifetime $t:ty) => {
        impl<$life> TryFrom<&$life $t> for NormpathBuf {
            type Error = ConvertError<&$life $t>;

            /// Validates that `value` is normalized to create a [`NormpathBuf`]
            /// from it.
            fn try_from(value: &$life $t) -> Result<Self, Self::Error> {
                imp::normalize_new_buf(PathBuf::from(value))
                    .map_err(|e| ConvertError::new(e.error, value))
            }
        }
    };
}

impl_try_from!(owned over <> String);
impl_try_from!(owned over <> OsString);
impl_try_from!(owned over <> PathBuf);
impl_try_from!(owned over <> Box<Path>);
impl_try_from!(owned over <'a> Cow<'a, Path>);

impl_try_from!(&'a str);
impl_try_from!(&'a String);
impl_try_from!(&'a OsStr);
impl_try_from!(&'a OsString);
impl_try_from!(&'a Path);
impl_try_from!(&'a PathBuf);

#[cfg(feature = "serde")]
#[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
impl Serialize for Normpath {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_path().serialize(serializer)
    }
}

#[cfg(feature = "serde")]
#[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
impl Serialize for NormpathBuf {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.as_path().serialize(serializer)
    }
}

impl FromStr for NormpathBuf {
    type Err = ConvertError<String>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        imp::normalize_new_buf(s.to_string())
    }
}

impl FromStr for Box<Normpath> {
    type Err = ConvertError<String>;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        imp::normalize_new_buf(s.to_string()).map(|p| p.into_boxed_path())
    }
}

#[cfg(feature = "serde")]
#[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
impl<'a, 'de: 'a> Deserialize<'de> for &'a Normpath {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let path = <&Path>::deserialize(deserializer)?;
        imp::validate(path).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "serde")]
#[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
impl<'de> Deserialize<'de> for NormpathBuf {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let path = PathBuf::deserialize(deserializer)?;
        imp::normalize_new_buf(path).map_err(serde::de::Error::custom)
    }
}

#[cfg(feature = "serde")]
#[cfg_attr(docsrs, doc(cfg(feature = "serde")))]
impl<'de> Deserialize<'de> for Box<Normpath> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let path = PathBuf::deserialize(deserializer)?;
        imp::normalize_new_buf(path)
            .map(|p| p.into_boxed_path())
            .map_err(serde::de::Error::custom)
    }
}

/// Normalizes all non-canonical patterns of `path`.
///
/// This function does not try to remove parent components on Unix, but does so
/// on Windows. See [crate documentation](crate) for more details about that.
///
/// Consider using [`NormpathBuf`] (or [`Normpath`]) if validation should be
/// type-checked and thus enforced by the compiler.
///
/// # Examples
///
/// ```rust,ignore-windows
/// # use normal_path::canonicalize_lexically;
/// use std::path::PathBuf;
///
/// let path1 = PathBuf::from("//foo/./bar/");
/// let norm1 = canonicalize_lexically(path1);
/// assert_eq!(&norm1, "/foo/bar");
///
/// let path2 = PathBuf::from(".//foo/../bar/");
/// let norm2 = canonicalize_lexically(path2);
/// assert_eq!(&norm2, "foo/../bar");
/// ```
///
/// Windows deals with parent components differently:
///
/// ```rust
/// # #[cfg(windows)] {
/// # use normal_path::canonicalize_lexically;
/// use std::path::PathBuf;
///
/// let path1 = PathBuf::from(r"C:\foo/.\bar\");
/// let norm1 = canonicalize_lexically(path1);
/// assert_eq!(&norm1, r"C:\foo\bar");
///
/// let path2 = PathBuf::from(r"./\foo\../bar\..");
/// let norm2 = canonicalize_lexically(path2);
/// assert_eq!(&norm2, "");
/// # }
/// ```
#[must_use]
#[inline]
pub fn canonicalize_lexically(path: PathBuf) -> PathBuf {
    NormpathBuf::normalize(path)
        .map(|it| it.into_path_buf())
        .unwrap_or_else(|e| e.value)
}

#[cfg(test)]
mod tests {
    use std::{iter, ops::RangeBounds, path::Component};

    use fastrand::Rng;

    use crate::draw;

    use super::{ConvertError as Ec, Error as E, *};

    const MIN_ABS: usize = if cfg!(unix) { 1 } else { 3 };

    #[cfg(unix)]
    fn make_root() -> NormpathBuf {
        NormpathBuf::root()
    }

    #[cfg(windows)]
    fn make_root() -> NormpathBuf {
        let letter = char::from(fastrand::u8(b'A'..=b'Z'));
        NormpathBuf::try_from(format!(r"{letter}:\")).unwrap()
    }

    fn make_paths(
        bound: impl RangeBounds<usize> + Clone,
        mut draw: impl FnMut(usize) -> String,
    ) -> impl Iterator<Item = String> {
        let mut rng = Rng::new();
        iter::from_fn(move || Some(draw(rng.usize(bound.clone()))))
    }

    #[cfg(unix)]
    fn to_canonical(path: &Path) -> PathBuf {
        path.components()
            .filter(|c| !matches!(c, Component::CurDir))
            .collect()
    }

    #[cfg(windows)]
    fn to_canonical(path: &Path) -> PathBuf {
        use std::path::Prefix;

        let mut out = PathBuf::with_capacity(path.as_os_str().len());
        for component in path.components() {
            use {std::path::Prefix::*, Component::*};
            match component {
                Prefix(component) => match component.kind() {
                    Disk(c) => {
                        let letter = char::from(c.to_ascii_uppercase());
                        out.push(format!("{letter}:"));
                    }
                    DeviceNS(name) => {
                        let name = name.to_str().unwrap();
                        out.push(format!(r"\\.\{name}"));
                    }
                    UNC(server, share) => {
                        let server = server.to_str().unwrap();
                        let share = share.to_str().unwrap();
                        out.push(format!(r"\\{server}\{share}"));
                    }
                    _ => return path.into(),
                },
                RootDir => out.push("\\"),
                CurDir => continue,
                ParentDir if out.file_name().is_some() => assert!(out.pop()),
                ParentDir => out.push(".."),
                Normal(name) => out.push(name),
            }
        }

        let mut components = out.components();
        let first = components.next();
        let tail = components.as_path();

        let is_phony = match first {
            Some(Component::Prefix(prefix)) => {
                matches!(prefix.kind(), Prefix::DeviceNS(_) | Prefix::UNC(_, _))
                    && tail.as_os_str() == "\\"
            }
            _ => false,
        };

        if is_phony {
            let mut bytes = std::mem::take(&mut out)
                .into_os_string()
                .into_encoded_bytes();

            bytes.pop();
            out = unsafe { OsString::from_encoded_bytes_unchecked(bytes) }.into();
        }

        out
    }

    fn is_canonical(path: &Path) -> bool {
        path.as_os_str() == to_canonical(path).as_os_str()
    }

    #[cfg(unix)]
    fn is_parentless(path: &Path) -> bool {
        path.components()
            .all(|c| !matches!(c, Component::ParentDir))
    }

    #[cfg(windows)]
    fn is_parentless(path: &Path) -> bool {
        use Component::*;

        let mut components = path.components();
        let start = match components.next() {
            Some(Prefix(it)) if it.kind().is_verbatim() => return true,
            Some(ParentDir) => return false,
            Some(Normal(_)) => 1u32,
            Some(_) => 0u32,
            _ => return true,
        };

        path.components()
            .map(|c| match c {
                ParentDir => -1,
                Normal(_) => 1,
                _ => 0,
            })
            .try_fold(start, |acc, step| acc.checked_add_signed(step))
            .is_some()
    }

    fn is_normalized(path: &Path) -> bool {
        path.is_absolute() && is_canonical(path) && is_parentless(path)
    }

    fn into_source<T>(error: Ec<T>) -> E {
        error.error
    }

    #[cfg(unix)]
    #[test]
    pub fn root() {
        Normpath::validate("/").unwrap();
        NormpathBuf::try_from("/").unwrap();
    }

    #[cfg(windows)]
    #[test]
    pub fn root() {
        let paths = ('A'..='Z').map(|c| format!(r"{c}:\"));
        for path in paths {
            Normpath::validate(&path).unwrap();
            NormpathBuf::try_from(path).unwrap();
        }
    }

    #[test]
    pub fn empty() {
        assert_eq!(Normpath::validate(""), Err(E::NotAbsolute));
        assert_eq!(
            NormpathBuf::try_from("").map_err(into_source),
            Err(E::NotAbsolute),
        );
    }

    fn test_normalize(path: &str) -> OsString {
        let in_place = NormpathBuf::normalize(path.into()).map(|p| p.into_os_string());
        let new = Normpath::normalize(path).map(|p| p.into_owned().into_os_string());
        match (in_place, new) {
            (Ok(in_place), Ok(new)) => {
                assert_eq!(in_place, new, "inconsistent on {:?}", path);
                in_place
            }
            (Err(ec), Err(_)) => ec.value.into(),
            (a, b) => panic!(
                "inconsistent on {:?}: in-place = {:?}, new = {:?}",
                path, a, b
            ),
        }
    }

    #[cfg(unix)]
    #[test]
    pub fn example_normalize() {
        assert_eq!(test_normalize("/foo/bar"), "/foo/bar");
        assert_eq!(test_normalize("//foo/./..//bar//"), "/foo/../bar");

        assert_eq!(test_normalize("foo/bar"), "foo/bar");
        assert_eq!(test_normalize(".//foo/.//bar/..//"), "foo/bar/..");
    }

    #[cfg(windows)]
    #[test]
    pub fn example_normalize() {
        assert_eq!(test_normalize(r"C:\foo\bar"), r"C:\foo\bar");
        assert_eq!(test_normalize(r"c:/foo/.."), r"C:\");
        assert_eq!(test_normalize(r"c:\/foo\..\./../bar/.."), r"C:\..");

        assert_eq!(test_normalize(r"foo\bar"), r"foo\bar");
        assert_eq!(test_normalize(r".\/foo\./\../"), r"");
        assert_eq!(test_normalize(r".\/foo/..\bar/../..//"), r"..");

        assert_eq!(test_normalize(r"\/.\dev\foo"), r"\\.\dev\foo");
        assert_eq!(test_normalize(r"\\./dev"), r"\\.\dev");
        assert_eq!(test_normalize(r"//./dev/foo\/bar\../"), r"\\.\dev\foo");

        assert_eq!(test_normalize(r"\/s\s\foo\bar"), r"\\s\s\foo\bar");
        assert_eq!(test_normalize(r"\\s/s\foo/\.\..\bar\/"), r"\\s\s\bar");
        assert_eq!(test_normalize(r"//s\s/foo\../\./../\bar/.."), r"\\s\s\..");

        assert_eq!(test_normalize(r"C:foo\bar"), r"C:foo\bar");
        assert_eq!(test_normalize(r"c:.\\foo\../\bar//"), r"C:bar");
        assert_eq!(test_normalize(r"c:.\foo\../bar/.."), r"C:");

        // No normalization will ever be applied to verbatim paths.
        assert_eq!(
            test_normalize(r"\\?\C:\foo/..\/bar/."),
            r"\\?\C:\foo/..\/bar/."
        );
        assert_eq!(
            test_normalize(r"\\?\UNC\s\s\foo\./..\bar/"),
            r"\\?\UNC\s\s\foo\./..\bar/"
        );
    }

    #[test]
    pub fn normalize() {
        let paths = make_paths(1..64, draw::common);
        for path in paths.take(1024) {
            let reference = to_canonical(Path::new(&path));
            let ours = test_normalize(&path);

            assert_eq!(reference.as_os_str(), ours, "original: {path:?}");
        }
    }

    #[cfg(unix)]
    fn test_push(subject: &mut NormpathBuf, reference: &mut PathBuf, component: &str) {
        if is_parentless(component.as_ref()) {
            reference.push(component);
            subject.push(component).unwrap();

            assert_eq!(subject.as_path(), reference.as_path());
        } else {
            let copy = subject.clone();
            let error = subject.push(component).unwrap_err();
            assert_eq!(error, E::ContainsParent);
            assert_eq!(*subject, copy);
        }
    }

    #[cfg(windows)]
    fn test_push(subject: &mut NormpathBuf, reference: &mut PathBuf, component: &str) {
        let peek = reference.join(component);
        if peek.is_absolute() && is_parentless(&peek) {
            *reference = to_canonical(&peek);
            subject.push(component).unwrap();

            assert_eq!(subject.as_path(), reference.as_path());
        } else {
            let copy = subject.clone();
            let error = subject.push(component).unwrap_err();
            if !peek.is_absolute() {
                assert_eq!(error, E::NotAbsolute);
            } else {
                assert_eq!(error, E::ContainsParent);
            }
            assert_eq!(*subject, copy);
        }
    }

    #[test]
    pub fn push_relative() {
        for _ in 0..128 {
            let components = make_paths(1..16, draw::relative);

            let mut path = make_root();
            let mut twin = path.clone().into_path_buf();
            for component in components.take(64) {
                test_push(&mut path, &mut twin, &component);
            }
        }
    }

    #[test]
    pub fn push_absolute() {
        for _ in 0..128 {
            let components = make_paths(MIN_ABS..32, draw::absolute);

            let mut path = make_root();
            let mut twin = path.clone().into_path_buf();
            for component in components.take(32) {
                test_push(&mut path, &mut twin, &component);
            }
        }
    }

    #[cfg(windows)]
    #[test]
    pub fn push_partial() {
        for path in make_paths(MIN_ABS..64, draw::normal).take(128) {
            let roots = make_paths(1..32, draw::root_only);

            let mut path = NormpathBuf::try_from(path).unwrap();
            let mut twin = path.clone().into_path_buf();
            for root in roots.take(32) {
                test_push(&mut path, &mut twin, &root);
            }
        }

        for path in make_paths(MIN_ABS..64, draw::normal).take(64) {
            let prefixes = iter::from_fn(|| Some(draw::disk_only()));

            let mut path = NormpathBuf::try_from(path).unwrap();
            let mut twin = path.clone().into_path_buf();
            for prefix in prefixes.take(16) {
                test_push(&mut path, &mut twin, &prefix);
            }
        }
    }

    #[cfg(unix)]
    fn make_normal_paths() -> impl Iterator<Item = String> {
        make_paths(MIN_ABS..64, draw::normal).take(256)
    }

    #[cfg(windows)]
    fn make_normal_paths() -> impl Iterator<Item = String> {
        let genuine = make_paths(MIN_ABS..64, draw::normal);
        let verbatim = make_paths(7..64, draw::verbatim);
        genuine.take(224).chain(verbatim.take(32))
    }

    fn test_normal(path: &str) {
        let ref_validated = Normpath::validate(path).unwrap();
        assert!(is_normalized(ref_validated.as_path()));
        assert_eq!(ref_validated, Path::new(path));

        let ref_validated_c = Normpath::validate_canonical(path).unwrap();
        assert_eq!(ref_validated_c, ref_validated);

        let ref_validated_p = Normpath::validate_parentless(path).unwrap();
        assert_eq!(ref_validated_p, ref_validated);

        let ref_normalized = Normpath::normalize(path).unwrap();
        assert_eq!(&*ref_normalized, ref_validated);

        let buf_validated = NormpathBuf::validate(path).unwrap();
        assert_eq!(&buf_validated, ref_validated);

        let buf_normalized = NormpathBuf::normalize(PathBuf::from(path)).unwrap();
        assert_eq!(&buf_normalized, ref_validated);

        let buf_converted = NormpathBuf::try_from(path).unwrap();
        assert_eq!(&buf_converted, ref_validated);
    }

    #[cfg(unix)]
    #[test]
    pub fn examples_normal() {
        test_normal("/");
        test_normal("/foo/bar");
    }

    #[cfg(windows)]
    #[test]
    pub fn examples_normal() {
        test_normal(r"C:\");
        test_normal(r"C:\foo\bar");

        test_normal(r"\\.\dev");
        test_normal(r"\\.\dev\foo\bar");
        test_normal(r"\\s\s");
        test_normal(r"\\s\s\foo\bar");

        test_normal(r"\\?\C:.\../foo\bar//");
        test_normal(r"\\?\UNC\s\s/foo\/.\..\bar\/");
    }

    #[test]
    pub fn normal() {
        for path in make_normal_paths() {
            test_normal(&path);
        }
    }

    fn test_err_single(path: &str, error: E) {
        assert_eq!(Normpath::validate(&path), Err(error));
        assert_eq!(Normpath::validate_canonical(&path), Err(error));
        assert_eq!(Normpath::validate_parentless(&path), Err(error));

        assert_eq!(
            NormpathBuf::validate(&path).map_err(into_source),
            Err(error),
        );
        assert_eq!(
            NormpathBuf::normalize(PathBuf::from(&path)).map_err(into_source),
            Err(error),
        );

        let conv_err = NormpathBuf::try_from(path).unwrap_err();
        assert_eq!(conv_err.error, error);
        assert_eq!(conv_err.value, path);
    }

    fn test_err_noncanonical(path: &str, canonical: impl AsRef<Path>) {
        assert_eq!(Normpath::validate(&path), Err(E::NotCanonical));
        assert_eq!(Normpath::validate_canonical(&path), Err(E::NotCanonical));
        assert_eq!(Normpath::validate_parentless(&path), Err(E::NotCanonical));

        assert_eq!(
            NormpathBuf::validate(&path).map_err(into_source),
            Err(E::NotCanonical),
        );

        let ref_normalized = Normpath::normalize(&path).unwrap();
        assert_eq!(&*ref_normalized, canonical.as_ref());

        let buf_normalized = NormpathBuf::normalize(PathBuf::from(&path)).unwrap();
        assert_eq!(&buf_normalized, canonical.as_ref());

        let buf_converted = NormpathBuf::try_from(path.to_string()).unwrap();
        assert_eq!(&buf_converted, canonical.as_ref());
    }

    #[cfg(unix)]
    #[test]
    pub fn examples_err_single_relative() {
        test_err_single("", E::NotAbsolute);
        test_err_single("foo/bar", E::NotAbsolute);
    }

    #[cfg(windows)]
    #[test]
    pub fn examples_err_single_relative() {
        test_err_single("", E::NotAbsolute);
        test_err_single(r"foo\bar", E::NotAbsolute);

        test_err_single(r"\", E::NotAbsolute);
        test_err_single(r"\foo\bar", E::NotAbsolute);

        test_err_single(r"C:", E::NotAbsolute);
        test_err_single(r"C:foo\bar", E::NotAbsolute);
    }

    #[test]
    pub fn err_single_relative() {
        let paths = make_paths(1..64, draw::relative)
            .filter(|p| is_parentless(p.as_ref()) && is_canonical(p.as_ref()));

        for path in paths.take(256) {
            test_err_single(&path, E::NotAbsolute);
        }
    }

    #[cfg(unix)]
    #[test]
    pub fn examples_err_single_parent() {
        test_err_single("/..", E::ContainsParent);
        test_err_single("/foo/../bar", E::ContainsParent);
    }

    #[cfg(windows)]
    #[test]
    pub fn examples_err_single_parent() {
        test_err_single(r"C:\..", E::ContainsParent);

        test_err_single(r"\\.\dev\..", E::ContainsParent);
        test_err_single(r"\\s\s\..", E::ContainsParent);
    }

    #[test]
    pub fn err_single_parent() {
        let paths = make_paths(MIN_ABS..64, draw::absolute)
            .filter(|p| !is_parentless(Path::new(p)) && is_canonical(Path::new(p)));

        for path in paths.take(256) {
            test_err_single(&path, E::ContainsParent);
        }
    }

    #[cfg(unix)]
    #[test]
    pub fn examples_err_single_noncanonical() {
        test_err_noncanonical("//", "/");
        test_err_noncanonical("/.", "/");
        test_err_noncanonical("/foo//bar", "/foo/bar");
        test_err_noncanonical("/foo/./bar", "/foo/bar");
        test_err_noncanonical("/foo/bar/", "/foo/bar");
        test_err_noncanonical("/.../..../", "/.../....");
    }

    #[cfg(windows)]
    #[test]
    pub fn examples_err_single_noncanonical() {
        test_err_noncanonical(r"c:\", r"C:\");
        test_err_noncanonical(r"C:\\", r"C:\");
        test_err_noncanonical(r"C:\.", r"C:\");
        test_err_noncanonical(r"C:\foo\..", r"C:\");
        test_err_noncanonical(r"C:/foo/bar", r"C:\foo\bar");
        test_err_noncanonical(r"C:\foo\\bar", r"C:\foo\bar");
        test_err_noncanonical(r"C:\foo\.\bar", r"C:\foo\bar");
        test_err_noncanonical(r"C:\foo\bar\", r"C:\foo\bar");
        test_err_noncanonical(r"C:/.../....", r"C:\...\....");

        test_err_noncanonical(r"\\.\dev\\", r"\\.\dev\");
        test_err_noncanonical(r"\\.\dev\.", r"\\.\dev\");
        test_err_noncanonical(r"\\s\s\\", r"\\s\s\");
        test_err_noncanonical(r"\\s\s\.", r"\\s\s\");
    }

    #[test]
    pub fn err_single_noncanonical() {
        let paths = make_paths(MIN_ABS..64, draw::absolute)
            .filter(|p| !is_canonical(Path::new(p)) && is_parentless(Path::new(p)));

        for path in paths.take(256) {
            test_err_noncanonical(&path, to_canonical(path.as_ref()));
        }
    }

    fn test_err_preference(path: &str) {
        assert_eq!(Normpath::validate_canonical(path), Err(E::NotCanonical));
        assert_eq!(Normpath::validate_parentless(path), Err(E::ContainsParent));
    }

    #[cfg(unix)]
    #[test]
    pub fn examples_err_preference() {
        test_err_preference("/foo/../bar/");
        test_err_preference("//foo/../bar");
    }

    #[cfg(windows)]
    #[test]
    pub fn examples_err_preference() {
        test_err_preference(r"C:\..\foo\bar\");
        test_err_preference(r"C:\\foo\..\..\bar");
        test_err_preference(r"C:\foo\..\..");

        test_err_preference(r"\\.\dev\..\foo\bar\");
        test_err_preference(r"\\.\dev\\foo\..\..\bar");
        test_err_preference(r"\\s\s\..\foo\bar\");
        test_err_preference(r"\\s\s\\foo\..\..\bar");
    }

    #[test]
    pub fn err_preference() {
        let paths = make_paths(MIN_ABS..64, draw::absolute)
            .filter(|p| !is_canonical(Path::new(p)) && !is_parentless(Path::new(p)));

        for path in paths.take(256) {
            test_err_preference(&path);
        }
    }
}
