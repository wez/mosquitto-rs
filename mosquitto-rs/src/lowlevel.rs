use crate::Error;
pub(crate) use libmosquitto_sys as sys;
use std::convert::TryInto;
use std::ffi::{CStr, CString};
use std::os::raw::{c_int, c_void};
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
    ) -> Result<MessageId, Error> {
        let mut mid = 0;
        let err = unsafe {
            sys::mosquitto_publish(
                self.m,
                &mut mid,
                cstr(topic)?.as_ptr(),
                payload
                    .len()
                    .try_into()
                    .map_err(|_| Error::Mosq(sys::mosq_err_t::MOSQ_ERR_PAYLOAD_SIZE))?,
                payload.as_ptr() as *const _,
                qos as c_int,
                retain,
            )
        };
        Error::result(err, mid)
    }

    pub fn subscribe(&mut self, pattern: &str, qos: QoS) -> Result<MessageId, Error> {
        let mut mid = 0;
        let err = unsafe {
            sys::mosquitto_subscribe(self.m, &mut mid, cstr(pattern)?.as_ptr(), qos as _)
        };
        Error::result(err, mid)
    }

    pub fn set_callbacks<C: Callbacks + 'static>(&mut self, cb: C) {
        let cb = CallbackWrapper { cb: Box::new(cb) };
        // Double-box to avoid the compiler complaining about casting trait
        // pointers when subsequently calling Box::from_raw
        let cb: Box<CallbackWrapper> = Box::new(cb);
        let cb: *mut CallbackWrapper = Box::into_raw(cb);
        unsafe {
            // Mosq now owns cb
            sys::mosquitto_user_data_set(self.m, cb as *mut c_void);

            sys::mosquitto_connect_callback_set(self.m, Some(CallbackWrapper::connect));
            sys::mosquitto_disconnect_callback_set(self.m, Some(CallbackWrapper::disconnect));
            sys::mosquitto_publish_callback_set(self.m, Some(CallbackWrapper::publish));
            sys::mosquitto_subscribe_callback_set(self.m, Some(CallbackWrapper::subscribe));
            sys::mosquitto_message_callback_set(self.m, Some(CallbackWrapper::message));
        }
    }

    pub fn clear_callbacks(&mut self) {
        unsafe {
            let cb = sys::mosquitto_userdata(self.m) as *mut CallbackWrapper;
            if !cb.is_null() {
                let cb = Box::from_raw(cb);
                drop(cb);

                sys::mosquitto_user_data_set(self.m, std::ptr::null_mut());
            }
        }
    }

    pub fn loop_until_explicitly_disconnected(
        &mut self,
        timeout_milliseconds: c_int,
    ) -> Result<(), Error> {
        unsafe {
            let max_packets = 1;
            Error::result(
                sys::mosquitto_loop_forever(self.m, timeout_milliseconds, max_packets),
                (),
            )
        }
    }
}

struct CallbackWrapper {
    cb: Box<dyn Callbacks>,
}

fn with_transient_client<F: FnOnce(&mut Mosq)>(m: *mut sys::mosquitto, func: F) {
    let mut client = Mosq { m };
    func(&mut client);
    std::mem::forget(client);
}

impl CallbackWrapper {
    unsafe fn resolve_self<'a>(cb: *mut c_void) -> &'a mut CallbackWrapper {
        &mut *(cb as *mut CallbackWrapper)
    }

    unsafe extern "C" fn connect(m: *mut sys::mosquitto, cb: *mut c_void, rc: c_int) {
        let cb = Self::resolve_self(cb);
        with_transient_client(m, |client| {
            cb.cb.on_connect(client, rc);
        });
    }

    unsafe extern "C" fn disconnect(m: *mut sys::mosquitto, cb: *mut c_void, rc: c_int) {
        let cb = Self::resolve_self(cb);
        with_transient_client(m, |client| {
            cb.cb.on_disconnect(client, rc);
        });
    }

    unsafe extern "C" fn publish(m: *mut sys::mosquitto, cb: *mut c_void, mid: MessageId) {
        let cb = Self::resolve_self(cb);
        with_transient_client(m, |client| {
            cb.cb.on_publish(client, mid);
        });
    }

    unsafe extern "C" fn subscribe(
        m: *mut sys::mosquitto,
        cb: *mut c_void,
        mid: MessageId,
        qos_count: c_int,
        granted_qos: *const c_int,
    ) {
        let cb = Self::resolve_self(cb);
        with_transient_client(m, |client| {
            let granted_qos = std::slice::from_raw_parts(granted_qos, qos_count as usize);
            let granted_qos: Vec<QoS> = granted_qos.iter().map(QoS::from_int).collect();
            cb.cb.on_subscribe(client, mid, &granted_qos);
        });
    }

    unsafe extern "C" fn message(
        m: *mut sys::mosquitto,
        cb: *mut c_void,
        msg: *const sys::mosquitto_message,
    ) {
        let cb = Self::resolve_self(cb);
        with_transient_client(m, |client| {
            let msg = &*msg;
            let topic = CStr::from_ptr(msg.topic);
            let topic = topic.to_string_lossy().to_string();
            cb.cb.on_message(
                client,
                msg.mid,
                &topic,
                std::slice::from_raw_parts(msg.payload as *const u8, msg.payloadlen as usize),
                QoS::from_int(&msg.qos),
                msg.retain,
            );
        });
    }
}

pub type MessageId = c_int;

pub trait Callbacks {
    fn on_connect(&self, _client: &mut Mosq, _reason: c_int) {}
    fn on_disconnect(&self, _client: &mut Mosq, _reason: c_int) {}
    fn on_publish(&self, _client: &mut Mosq, _mid: MessageId) {}
    fn on_subscribe(&self, _client: &mut Mosq, _mid: MessageId, _granted_qos: &[QoS]) {}
    fn on_message(
        &self,
        _client: &mut Mosq,
        _mid: MessageId,
        _topic: &str,
        _payload: &[u8],
        _qos: QoS,
        _retain: bool,
    ) {
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QoS {
    AtMostOnce = 0,
    AtLeastOnce = 1,
    ExactlyOnce = 2,
}

impl QoS {
    fn from_int(i: &c_int) -> QoS {
        match i {
            0 => Self::AtMostOnce,
            1 => Self::AtLeastOnce,
            2 => Self::ExactlyOnce,
            _ => Self::ExactlyOnce,
        }
    }
}

impl Drop for Mosq {
    fn drop(&mut self) {
        unsafe {
            self.clear_callbacks();
            sys::mosquitto_destroy(self.m);
        }
    }
}
