use crate::mapper;
use crate::util::resolve_symlink;
use std::borrow::Borrow;
use std::ffi::OsString;
use std::io;
use std::ops::{self, Deref};
use std::path::StripPrefixError;
pub use std::path::{Path, PathBuf};
use std::sync::Mutex;

use once_cell::sync::Lazy;

pub use Path as HostPath;
pub use PathBuf as HostPathBuf;

pub static MAPPER: Lazy<Mutex<Mapper>> = Lazy::new(|| Mutex::new(Mapper::new()));

pub struct Mapper {
    mappings: Vec<(GuestPathBuf, HostPathBuf)>,
}

pub struct GuestPath {
    inner: Path,
}

pub struct GuestPathBuf {
    inner: PathBuf,
}

impl Mapper {
    fn new() -> Self {
        Self { mappings: vec![] }
    }

    fn host_to_guest<P: AsRef<HostPath>>(&self, p: P) -> io::Result<GuestPathBuf> {
        p.as_ref()
            .strip_prefix("/")
            .map(|p| GuestPath::new("/run/host").join(GuestPath::new(p)))
            .map_err(|_| io::Error::from(io::ErrorKind::InvalidInput))
    }

    fn guest_to_host<P: AsRef<GuestPath>>(&self, p: P) -> io::Result<HostPathBuf> {
        resolve_symlink(&self.mappings, p)
    }
}

impl GuestPath {
    pub fn new<P: ?Sized + AsRef<Path>>(p: &P) -> &GuestPath {
        unsafe { &*(p.as_ref() as *const Path as *const GuestPath) }
    }

    pub fn join<P: AsRef<GuestPath>>(&self, p: P) -> GuestPathBuf {
        GuestPathBuf::from_path_buf(self.inner.join(&p.as_ref().inner))
    }

    pub fn strip_prefix<P: AsRef<GuestPath>>(
        &self,
        base: P,
    ) -> Result<&GuestPath, StripPrefixError> {
        self.inner
            .strip_prefix(&base.as_ref().inner)
            .map(GuestPath::new)
    }

    pub fn to_host(&self) -> io::Result<HostPathBuf> {
        mapper!().guest_to_host(self)
    }

    pub fn as_path(&self) -> &Path {
        &self.inner
    }

    pub fn is_absolute(&self) -> bool {
        self.inner.is_absolute()
    }
}

impl GuestPathBuf {
    // pub fn new<P: ?Sized + AsRef<Path>>(p: &P) -> &Guest;

    fn from_path_buf(path: PathBuf) -> GuestPathBuf {
        GuestPathBuf { inner: path }
    }
}

impl AsRef<GuestPath> for OsString {
    fn as_ref(&self) -> &GuestPath {
        GuestPath::new(self)
    }
}

impl AsRef<GuestPath> for str {
    fn as_ref(&self) -> &GuestPath {
        GuestPath::new(self)
    }
}

impl AsRef<GuestPath> for String {
    fn as_ref(&self) -> &GuestPath {
        GuestPath::new(self)
    }
}

impl AsRef<GuestPath> for GuestPath {
    fn as_ref(&self) -> &GuestPath {
        self
    }
}

impl AsRef<GuestPath> for GuestPathBuf {
    fn as_ref(&self) -> &GuestPath {
        GuestPath::new(&self.inner)
    }
}

impl ops::Deref for GuestPathBuf {
    type Target = GuestPath;
    fn deref(&self) -> &GuestPath {
        GuestPath::new(&self.inner)
    }
}

impl Borrow<GuestPath> for GuestPathBuf {
    fn borrow(&self) -> &GuestPath {
        self.deref()
    }
}
