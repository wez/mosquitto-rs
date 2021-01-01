use crate::Error;
pub(crate) use libmosquitto_sys as sys;
use std::convert::TryInto;
use std::ffi::CString;
use std::os::raw::c_int;
use std::sync::Once;

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

pub struct Mosq {
    m: *mut sys::mosquitto,
}

// libmosquitto is internally thread safe, so tell the rust compiler
// that the Mosq wrapper type is Sync and Send.
unsafe impl Sync for Mosq {}
unsafe impl Send for Mosq {}

impl Mosq {
    /// Create a new client instance with a random client id
    pub fn with_auto_id() -> Result<Self, Error> {
        init_library();
        unsafe {
            let m = sys::mosquitto_new(std::ptr::null(), true, std::ptr::null_mut());
            if m.is_null() {
                Err(Error::Create(std::io::Error::last_os_error()))
            } else {
                Ok(Self { m })
            }
        }
    }

    /// Create a new client instance with the specified id.
    /// If clean_session is true, instructs the broker to clean all messages
    /// and subscriptions on disconnect.  Otherwise it will preserve them.
    pub fn with_id(id: &str, clean_session: bool) -> Result<Self, Error> {
        init_library();
        unsafe {
            let m = sys::mosquitto_new(cstr(id)?.as_ptr(), clean_session, std::ptr::null_mut());
            if m.is_null() {
                Err(Error::Create(std::io::Error::last_os_error()))
            } else {
                Ok(Self { m })
            }
        }
    }

    pub fn set_username_and_password(
        &mut self,
        username: Option<&str>,
        password: Option<&str>,
    ) -> Result<(), Error> {
        let user;
        let pass;
        let username = match username {
            Some(u) => {
                user = cstr(u)?;
                user.as_ptr()
            }
            None => std::ptr::null(),
        };

        let password = match password {
            Some(p) => {
                pass = cstr(p)?;
                pass.as_ptr()
            }
            None => std::ptr::null(),
        };

        let err = unsafe { sys::mosquitto_username_pw_set(self.m, username, password) };

        Error::result(err, ())
    }

    pub fn connect(
        &mut self,
        host: &str,
        port: c_int,
        keep_alive_seconds: c_int,
        bind_address: Option<&str>,
    ) -> Result<(), Error> {
        let host = cstr(host)?;
        let ba;
        let bind_address = match bind_address {
            Some(b) => {
                ba = cstr(b)?;
                ba.as_ptr()
            }
            None => std::ptr::null(),
        };
        let err = unsafe {
            sys::mosquitto_connect_bind(
                self.m,
                host.as_ptr(),
                port,
                keep_alive_seconds,
                bind_address,
            )
        };
        Error::result(err, ())
    }

    pub fn reconnect(&mut self) -> Result<(), Error> {
        Error::result(unsafe { sys::mosquitto_reconnect(self.m) }, ())
    }

    pub fn disconnect(&mut self) -> Result<(), Error> {
        Error::result(unsafe { sys::mosquitto_disconnect(self.m) }, ())
    }

    pub fn publish(
        &mut self,
        topic: &str,
        payload: &[u8],
        qos: QoS,
        retain: bool,
    ) -> Result<c_int, Error> {
        let mut mid = 0;
        let err = unsafe {
            sys::mosquitto_publish(
                self.m,
                &mut mid,
                cstr(topic)?.as_ptr(),
                payload
                    .len()
                    .try_into()
                    .map_err(|_| Error::Mosq(sys::mosq_err_t::MOSQ_ERR_OVERSIZE_PACKET))?,
                payload.as_ptr() as *const _,
                qos as c_int,
                retain,
            )
        };
        Error::result(err, mid)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QoS {
    AtMostOnce = 0,
    AtLeastOnce = 1,
    ExactlyOnce = 2,
}

impl Drop for Mosq {
    fn drop(&mut self) {
        unsafe {
            sys::mosquitto_destroy(self.m);
        }
    }
}
