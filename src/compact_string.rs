//! A compact string representation for use with label names and values.
//!
//! Label names and values are commonly small strings, which can be stored
//! efficiently on the stack if they are below a certain size. This module
//! provides the [`CompactString`] type which makes use of this optimization
//! to avoid having to heap allocate in the common case.
//!
//! The current implementation effectively reuses [`compact_str::CompactString`].
//! We use a newtype wrapper so we can implement the required pyo3 traits and
//! to make it easier to modify later if we need to.

use pyo3::{prelude::*, types::PyString};
use serde::{Deserialize, Serialize};

/// Wrapper around a [`compact_str::CompactString`] which also implements
/// [`pyo3::IntoPyObject`].
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Deserialize, Serialize)]
pub struct CompactString(compact_str::CompactString);

impl CompactString {
    /// Create a new [`CompactString`] from a static string.
    pub const fn const_new(s: &'static str) -> Self {
        Self(compact_str::CompactString::const_new(s))
    }

    /// Create a new [`CompactString`] from a string slice.
    pub fn new(s: &str) -> Self {
        Self(compact_str::CompactString::new(s))
    }
}

impl From<compact_str::CompactString> for CompactString {
    fn from(s: compact_str::CompactString) -> Self {
        Self(s)
    }
}

impl std::ops::Deref for CompactString {
    type Target = compact_str::CompactString;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl std::borrow::Borrow<str> for CompactString {
    fn borrow(&self) -> &str {
        &self.0
    }
}

impl<'py> IntoPyObject<'py> for CompactString {
    type Target = PyString;

    type Output = Bound<'py, Self::Target>;

    type Error = std::convert::Infallible;

    fn into_pyobject(self, py: Python<'py>) -> Result<Self::Output, Self::Error> {
        self.as_str().into_pyobject(py)
    }
}
