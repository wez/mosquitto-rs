use libmosquitto_sys as sys;
use crate::Error;
use std::sync::Once;
use std::ffi::CString;
use std::os::raw::c_int;

static INIT: Once = Once::new();

fn init_library() {
    // Note: we never call mosquitto_lib_cleanup as we can't ever
    // know when it will be safe to do so.
    INIT.call_once(|| unsafe {
        sys::mosquitto_lib_init();
    });
}

#[derive(Copy, Clone, Debug, Eq, PartialEq)]
pub struct LibraryVersion {
    /// The major version of the library
    pub major: c_int,
    /// The minor version of the library
    pub minor: c_int,
    /// The revision of the library
    pub revision: c_int,
    /// A unique number based on the major, minor and revision values
    pub version: c_int,
}

impl std::fmt::Display for LibraryVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.minor, self.major, self.revision)
    }
}

/// Returns the version information for the linked mosquitto library
pub fn lib_version() -> LibraryVersion {
    init_library();

    let mut vers = LibraryVersion {
        major: 0,
        minor: 0,
        revision: 0,
        version: 0,
    };
    unsafe {
        vers.version =
            sys::mosquitto_lib_version(&mut vers.major, &mut vers.minor, &mut vers.revision);
    }
    vers
}

pub(crate) fn cstr(s: &str) -> Result<CString, Error> {
    Ok(CString::new(s)?)
}

pub(crate) struct Mosq {
    m: *mut sys::mosquitto
}

// libmosquitto is internally thread safe, so tell the rust compiler
// that the Mosq wrapper type is Sync and Send.
unsafe impl Sync for Mosq {}
unsafe impl Send for Mosq {}

impl Mosq {
    fn create(id: &str, clean_session: bool) -> Result<Self, Error> {
        unsafe {
            let m = sys::mosquitto_new(cstr(id)?.as_ptr(), clean_session, std::ptr::null_mut());
            if m.is_null() {
                Err(Error::Create(std::io::Error::last_os_error()))
            } else {
                // Ensure that it really is thread safe
                sys::mosquitto_threaded_set(m, true);
                Ok(Self{m})
            }
        }
    }
}

impl Drop for Mosq {
    fn drop(&mut self) {
        unsafe {
            sys::mosquitto_destroy(self.m);
        }
    }
}
