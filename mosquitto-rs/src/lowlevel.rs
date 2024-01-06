use crate::Error;
pub(crate) use libmosquitto_sys as sys;
use std::convert::TryInto;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::path::Path;
use std::sync::Arc;
use std::sync::Once;
use std::time::Duration;

static INIT: Once = Once::new();

fn init_library() {
    // Note: we never call mosquitto_lib_cleanup as we can't ever
    // know when it will be safe to do so.
    INIT.call_once(|| unsafe {
        sys::mosquitto_lib_init();
    });
}

/// Represents the version of the linked mosquitto client library
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

/// `Mosq` is the low-level mosquitto client.
/// You probably want to look at [Client](struct.Client.html) instead.
pub struct Mosq<CB = ()>
where
    CB: Callbacks + Send + Sync,
{
    m: *mut sys::mosquitto,
    cb: Option<Arc<CallbackWrapper<CB>>>,
}

// libmosquitto is internally thread safe, so tell the rust compiler
// that the Mosq wrapper type is Sync and Send.
unsafe impl<CB: Callbacks + Send + Sync> Sync for Mosq<CB> {}
unsafe impl<CB: Callbacks + Send + Sync> Send for Mosq<CB> {}

impl<CB: Callbacks + Send + Sync> Mosq<CB> {
    /// Create a new client instance with a random client id
    pub fn with_auto_id(callbacks: CB) -> Result<Self, Error> {
        init_library();
        unsafe {
            let cb = Arc::new(CallbackWrapper::new(callbacks));
            let m = sys::mosquitto_new(std::ptr::null(), true, Arc::as_ptr(&cb) as *mut _);
            if m.is_null() {
                Err(Error::Create(std::io::Error::last_os_error()))
            } else {
                Ok(Self::set_callbacks(Self { m, cb: Some(cb) }))
            }
        }
    }

    /// Create a new client instance with the specified id.
    /// If clean_session is true, instructs the broker to clean all messages
    /// and subscriptions on disconnect.  Otherwise it will preserve them.
    pub fn with_id(callbacks: CB, id: &str, clean_session: bool) -> Result<Self, Error> {
        init_library();
        unsafe {
            let cb = Arc::new(CallbackWrapper::new(callbacks));
            let m = sys::mosquitto_new(
                cstr(id)?.as_ptr(),
                clean_session,
                Arc::as_ptr(&cb) as *mut _,
            );
            if m.is_null() {
                Err(Error::Create(std::io::Error::last_os_error()))
            } else {
                Ok(Self::set_callbacks(Self { m, cb: Some(cb) }))
            }
        }
    }

    /// Configure the client with an optional username and password.
    /// The default is `None` for both.
    /// Whether you need to configure these credentials depends on the
    /// broker configuration.
    pub fn set_username_and_password(
        &self,
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

    /// Connect to the broker on the specified host and port.
    /// port is typically 1883 for mqtt, but it may be different
    /// in your environment.
    ///
    /// `keep_alive_interval` specifies the interval at which
    /// keepalive requests are sent.  mosquitto has a minimum value
    /// of 5 for this and will generate an error if you use a smaller
    /// value.
    ///
    /// `bind_address` can be used to specify the outgoing interface
    /// for the connection.
    pub fn connect(
        &self,
        host: &str,
        port: c_int,
        keep_alive_interval: Duration,
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
                keep_alive_interval
                    .as_secs()
                    .try_into()
                    .map_err(|_| Error::Mosq(sys::mosq_err_t::MOSQ_ERR_INVAL))?,
                bind_address,
            )
        };
        Error::result(err, ())
    }

    /// Connect to the broker on the specified host and port,
    /// but don't block for the connection portion.
    /// (Note that name resolution may still block!).
    ///
    /// The connection will be completed later by running the message loop
    /// using either `loop_until_explicitly_disconnected` or
    /// `start_loop_thread`.
    ///
    /// port is typically 1883 for mqtt, but it may be different
    /// in your environment.
    ///
    /// `keep_alive_interval` specifies the interval at which
    /// keepalive requests are sent.  mosquitto has a minimum value
    /// of 5 for this and will generate an error if you use a smaller
    /// value.
    ///
    /// `bind_address` can be used to specify the outgoing interface
    /// for the connection.
    pub fn connect_non_blocking(
        &self,
        host: &str,
        port: c_int,
        keep_alive_interval: Duration,
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
            sys::mosquitto_connect_bind_async(
                self.m,
                host.as_ptr(),
                port,
                keep_alive_interval
                    .as_secs()
                    .try_into()
                    .map_err(|_| Error::Mosq(sys::mosq_err_t::MOSQ_ERR_INVAL))?,
                bind_address,
            )
        };
        Error::result(err, ())
    }

    /// Reconnect a disconnected client using the same parameters
    /// as were originally used to connect it.
    pub fn reconnect(&self) -> Result<(), Error> {
        Error::result(unsafe { sys::mosquitto_reconnect(self.m) }, ())
    }

    /// Disconnect the client.
    /// This will cause the message loop to terminate.
    pub fn disconnect(&self) -> Result<(), Error> {
        Error::result(unsafe { sys::mosquitto_disconnect(self.m) }, ())
    }

    /// Publish a message to the specified topic.
    ///
    /// The payload size can be 0-283, 435 or 455 bytes; other values
    /// will generate an error result.
    ///
    /// `retain` will set the message to be retained by the broker,
    /// and delivered to new subscribers.
    ///
    /// Returns the assigned MessageId value for the publish.
    /// The publish may not complete immediately.
    /// Your `Callbacks::on_publish` handler will be called
    /// when it completes.
    pub fn publish(
        &self,
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

    /// Configure will information for a mosquitto instance.
    /// By default, clients do not have a will.
    /// This must be called before calling `connect`.
    ///
    /// The payload size can be 0-283, 435 or 455 bytes; other values
    /// will generate an error result.
    ///
    /// `retain` will set the message to be retained by the broker,
    /// and delivered to new subscribers.
    pub fn set_last_will(
        &self,
        topic: &str,
        payload: &[u8],
        qos: QoS,
        retain: bool,
    ) -> Result<(), Error> {
        let err = unsafe {
            sys::mosquitto_will_set(
                self.m,
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
        Error::result(err, ())
    }

    /// Remove a previously configured will.
    /// This must be called before calling connect
    pub fn clear_last_will(&self) -> Result<(), Error> {
        let err = unsafe { sys::mosquitto_will_clear(self.m) };
        Error::result(err, ())
    }

    /// Establish a subscription for topics that match `pattern`.
    ///
    /// Your `Callbacks::on_message` handler will be called as messages
    /// matching your subscription arrive.
    ///
    /// Returns the MessageId of the subscription request; the subscriptions
    /// won't be active until the broker has processed the request.
    /// You can use an `on_subscribe` handler to determine when that is ready.
    pub fn subscribe(&self, pattern: &str, qos: QoS) -> Result<MessageId, Error> {
        let mut mid = 0;
        let err = unsafe {
            sys::mosquitto_subscribe(self.m, &mut mid, cstr(pattern)?.as_ptr(), qos as _)
        };
        Error::result(err, mid)
    }

    /// Remove subscription(s) for topics that match `pattern`.
    pub fn unsubscribe(&self, pattern: &str) -> Result<MessageId, Error> {
        let mut mid = 0;
        let err = unsafe { sys::mosquitto_unsubscribe(self.m, &mut mid, cstr(pattern)?.as_ptr()) };
        Error::result(err, mid)
    }

    fn set_callbacks(self) -> Self {
        unsafe {
            sys::mosquitto_connect_callback_set(self.m, Some(CallbackWrapper::<CB>::connect));
            sys::mosquitto_disconnect_callback_set(self.m, Some(CallbackWrapper::<CB>::disconnect));
            sys::mosquitto_publish_callback_set(self.m, Some(CallbackWrapper::<CB>::publish));
            sys::mosquitto_subscribe_callback_set(self.m, Some(CallbackWrapper::<CB>::subscribe));
            sys::mosquitto_message_callback_set(self.m, Some(CallbackWrapper::<CB>::message));
            sys::mosquitto_unsubscribe_callback_set(
                self.m,
                Some(CallbackWrapper::<CB>::unsubscribe),
            );
            sys::mosquitto_log_callback_set(self.m, Some(bridge_logs));
        }
        self
    }

    /// Returns a reference to the callbacks previously registered
    /// during construction.
    pub fn get_callbacks(&self) -> &CB {
        &self
            .cb
            .as_ref()
            .expect("get_callbacks not to be called on a transient Mosq")
            .cb
    }

    /// Runs the message loop for the client.
    /// This method will not return until the client is explicitly
    /// disconnected via the `disconnect` method.
    ///
    /// `timeout` specifies the internal sleep duration between
    /// iterations.
    pub fn loop_until_explicitly_disconnected(&self, timeout: Duration) -> Result<(), Error> {
        unsafe {
            let max_packets = 1;
            Error::result(
                sys::mosquitto_loop_forever(
                    self.m,
                    timeout
                        .as_millis()
                        .try_into()
                        .map_err(|_| Error::Mosq(sys::mosq_err_t::MOSQ_ERR_INVAL))?,
                    max_packets,
                ),
                (),
            )
        }
    }

    /// Starts a new thread to run the message loop for the client.
    /// The thread will run until the client is disconnected,
    /// or until `stop_loop_thread` is called.
    pub fn start_loop_thread(&self) -> Result<(), Error> {
        unsafe { Error::result(sys::mosquitto_loop_start(self.m), ()) }
    }

    /// Stops the message loop thread started via `start_loop_thread`
    pub fn stop_loop_thread(&self, force_cancel: bool) -> Result<(), Error> {
        unsafe { Error::result(sys::mosquitto_loop_stop(self.m, force_cancel), ()) }
    }

    /// Sets an option with a string value
    pub fn set_string_option(&self, option: sys::mosq_opt_t, value: &str) -> Result<(), Error> {
        let err = unsafe { sys::mosquitto_string_option(self.m, option, cstr(value)?.as_ptr()) };
        Error::result(err, ())
    }

    /// Sets an option with an integer value
    pub fn set_int_option(&self, option: sys::mosq_opt_t, value: c_int) -> Result<(), Error> {
        // Ideally we'd use sys::mosquitto_int_option here, but it isn't present in 1.4
        let mut value = value;
        let err = unsafe {
            sys::mosquitto_opts_set(self.m, option, &mut value as *mut c_int as *mut c_void)
        };
        Error::result(err, ())
    }

    /// Sets a void* pointer option such as MOSQ_OPT_SSL_CTX.
    /// Unsafe because we can't know whether what is being passed really matches up.
    pub unsafe fn set_ptr_option(
        &self,
        option: sys::mosq_opt_t,
        value: *mut c_void,
    ) -> Result<(), Error> {
        let err = sys::mosquitto_void_option(self.m, option, value);
        Error::result(err, ())
    }

    /// Configures the TLS parameters for the client.
    ///
    /// `ca_file` is the path to a PEM encoded trust CA certificate file.
    /// Either `ca_file` or `ca_path` must be set.
    ///
    /// `ca_path` is the path to a directory containing PEM encoded trust
    /// CA certificates.  Either `ca_file` or `ca_path` must be set.
    ///
    /// `cert_file` path to a file containing the PEM encoded certificate
    /// file for this client.  If `None` then `key_file` must also be `None`
    /// and no client certificate will be used.
    ///
    /// `key_file` path to a file containing the PEM encoded private key
    /// for this client.  If `None` them `cert_file` must also be `None`
    /// and no client certificate will be used.
    ///
    /// `pw_callback` allows you to provide a password to decrypt an
    /// encrypted key file.  Specify `None` if the key file isn't
    /// password protected.
    pub fn configure_tls<CAFILE, CAPATH, CERTFILE, KEYFILE>(
        &self,
        ca_file: Option<CAFILE>,
        ca_path: Option<CAPATH>,
        cert_file: Option<CERTFILE>,
        key_file: Option<KEYFILE>,
        pw_callback: Option<PasswdCallback>,
    ) -> Result<(), Error>
    where
        CAFILE: AsRef<Path>,
        CAPATH: AsRef<Path>,
        CERTFILE: AsRef<Path>,
        KEYFILE: AsRef<Path>,
    {
        let ca_file = path_to_cstring(ca_file)?;
        let ca_path = path_to_cstring(ca_path)?;
        let cert_file = path_to_cstring(cert_file)?;
        let key_file = path_to_cstring(key_file)?;

        let err = unsafe {
            sys::mosquitto_tls_set(
                self.m,
                opt_cstring_to_ptr(&ca_file),
                opt_cstring_to_ptr(&ca_path),
                opt_cstring_to_ptr(&cert_file),
                opt_cstring_to_ptr(&key_file),
                pw_callback,
            )
        };

        Error::result(err, ())
    }

    /// Controls reconnection behavior when running in the message loop.
    /// By default, if a client is unexpectedly disconnected, mosquitto will
    /// try to reconnect.  The default reconnect parameters are to retry once
    /// per second to reconnect.
    ///
    /// You change adjust the delay between connection attempts by changing
    /// the parameters with this function.
    ///
    /// `reconnect_delay` is the base delay amount.
    ///
    /// If `use_exponential_backoff` is true, then the delay is doubled on
    /// each successive attempt, until the `max_reconnect_delay` is reached.
    ///
    /// If `use_exponential_backoff` is false, then the `reconnect_delay` is
    /// added on each successive attempt, until the `max_reconnect_delay` is
    /// reached.
    pub fn set_reconnect_delay(
        &self,
        reconnect_delay: Duration,
        max_reconnect_delay: Duration,
        use_exponential_backoff: bool,
    ) -> Result<(), Error> {
        let err = unsafe {
            sys::mosquitto_reconnect_delay_set(
                self.m,
                reconnect_delay
                    .as_secs()
                    .try_into()
                    .map_err(|_| Error::Mosq(sys::mosq_err_t::MOSQ_ERR_INVAL))?,
                max_reconnect_delay
                    .as_secs()
                    .try_into()
                    .map_err(|_| Error::Mosq(sys::mosq_err_t::MOSQ_ERR_INVAL))?,
                use_exponential_backoff,
            )
        };
        Error::result(err, ())
    }
}

fn opt_cstring_to_ptr(c: &Option<CString>) -> *const c_char {
    match c {
        Some(c) => c.as_ptr(),
        None => std::ptr::null(),
    }
}

fn path_to_cstring<P: AsRef<Path>>(p: Option<P>) -> Result<Option<CString>, Error> {
    match p {
        Some(p) => {
            let p = p.as_ref();

            #[cfg(unix)]
            {
                use std::os::unix::ffi::OsStrExt;
                let c = CString::new(p.as_os_str().as_bytes()).map_err(Error::CString)?;
                Ok(Some(c))
            }

            #[cfg(windows)]
            {
                // This isn't 100% correct, but it's probably good enough :-/
                let s = p
                    .to_str()
                    .ok_or(Error::Mosq(sys::mosq_err_t::MOSQ_ERR_MALFORMED_UTF8))?;
                let c = cstr(s)?;
                Ok(Some(c))
            }
        }
        None => Ok(None),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ReasonCode(pub c_int);

impl ReasonCode {
    /// Returns true if the reason represents an unexpected disconnect
    pub fn is_unexpected_disconnect(&self) -> bool {
        self.0 != 0
    }
}

impl std::fmt::Display for ReasonCode {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        let desc = unsafe { sys::mosquitto_reason_string(self.0) };
        if desc.is_null() {
            write!(fmt, "REASON code {}", self.0)
        } else {
            let desc = unsafe { CStr::from_ptr(desc) };
            write!(fmt, "REASON code {}: {}", self.0, desc.to_string_lossy())
        }
    }
}

impl std::fmt::Debug for ReasonCode {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, fmt)
    }
}

/// Represents the status of the connection attempt.
/// The embedded status code value depends on the protocol version
/// that was setup for the client.
/// For MQTT v5.0, look at section 3.2.2.2 Connect Reason code: <https://docs.oasis-open.org/mqtt/mqtt/v5.0/os/mqtt-v5.0-os.html>
/// For MQTT v3.1.1, look at section 3.2.2.3 Connect Return code: <http://docs.oasis-open.org/mqtt/mqtt/v3.1.1/mqtt-v3.1.1.html>
/// Use the `is_successful` method to test whether the connection was
/// successfully initiated.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct ConnectionStatus(pub c_int);

impl std::fmt::Display for ConnectionStatus {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        let desc = unsafe { sys::mosquitto_connack_string(self.0) };
        if desc.is_null() {
            write!(fmt, "CONNACK code {}", self.0)
        } else {
            let desc = unsafe { CStr::from_ptr(desc) };
            write!(fmt, "CONNACK code {}: {}", self.0, desc.to_string_lossy())
        }
    }
}

impl std::fmt::Debug for ConnectionStatus {
    fn fmt(&self, fmt: &mut std::fmt::Formatter) -> std::fmt::Result {
        std::fmt::Display::fmt(self, fmt)
    }
}

impl ConnectionStatus {
    /// Returns true if the connection attempt was successful.
    pub fn is_successful(&self) -> bool {
        self.0 == sys::mqtt311_connack_codes::CONNACK_ACCEPTED as c_int
    }
}

struct CallbackWrapper<T: Callbacks> {
    /// This used to be RefCell, but I've observed that the underlying
    /// library can make recursive dispatches to the callbacks,
    /// so we must not use any kind of lock or runtime checked
    /// borrow to guard access: we rely instead of this being
    /// immutable here and leaving it to the impl of Callbacks
    /// to appropriate scope any interior mutability
    cb: Box<T>,
}

fn with_transient_client<F: FnOnce(&mut Mosq)>(m: *mut sys::mosquitto, func: F) {
    let mut client = Mosq { m, cb: None };
    func(&mut client);
    std::mem::forget(client);
}

impl<T: Callbacks> CallbackWrapper<T> {
    fn new(cb: T) -> Self {
        Self { cb: Box::new(cb) }
    }

    unsafe fn resolve_self<'a>(cb: *mut c_void) -> &'a Self {
        &*(cb as *const Self)
    }

    unsafe extern "C" fn connect(m: *mut sys::mosquitto, cb: *mut c_void, rc: c_int) {
        let cb = Self::resolve_self(cb);
        with_transient_client(m, |client| {
            cb.cb.on_connect(client, ConnectionStatus(rc));
        });
    }

    unsafe extern "C" fn disconnect(m: *mut sys::mosquitto, cb: *mut c_void, rc: c_int) {
        let cb = Self::resolve_self(cb);
        with_transient_client(m, |client| {
            cb.cb.on_disconnect(client, ReasonCode(rc));
        });
    }

    unsafe extern "C" fn publish(m: *mut sys::mosquitto, cb: *mut c_void, mid: MessageId) {
        let cb = Self::resolve_self(cb);
        with_transient_client(m, |client| {
            cb.cb.on_publish(client, mid);
        });
    }

    unsafe extern "C" fn unsubscribe(m: *mut sys::mosquitto, cb: *mut c_void, mid: MessageId) {
        let cb = Self::resolve_self(cb);
        with_transient_client(m, |client| {
            cb.cb.on_unsubscribe(client, mid);
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
                topic,
                std::slice::from_raw_parts(msg.payload as *const u8, msg.payloadlen as usize),
                QoS::from_int(&msg.qos),
                msg.retain,
            );
        });
    }
}

/// Represents an individual message identifier.
/// This is used in this client to determine when a message
/// has been sent.
pub type MessageId = c_int;

/// An OpenSSL password callback (see `man SSL_CTX_set_default_passwd_cb_userdata`).
///
/// `buf` is the destination for the password string.
/// The size of `buf` is specified by `size`.
///
/// The goal of the callback is to obtain the password from the
/// user (or some credential store) and store it into `buf`.
///
/// The password must be NUL terminated.
/// The length of the password must be returned.
///
/// The other parameters to this function must be ignored as they
/// cannot be used safely in the Rust client binding.
///
/// ```no_run
/// use std::os::raw::{c_char, c_int, c_void};
/// use std::convert::TryInto;
///
/// unsafe extern "C" fn my_callback(
///     buf: *mut c_char,
///     size: c_int,
///     _: c_int,
///     _: *mut c_void
/// ) -> c_int {
///   let buf = std::slice::from_raw_parts_mut(buf as *mut u8, size as usize);
///
///   let pw = b"secret";
///   buf[..pw.len()].copy_from_slice(pw);
///   buf[pw.len()] = 0;
///   pw.len().try_into().unwrap()
/// }
/// ```
pub type PasswdCallback =
    unsafe extern "C" fn(buf: *mut c_char, size: c_int, _: c_int, _: *mut c_void) -> c_int;

/// Defines handlers that can be used to determine when various
/// functions have completed.
/// Take care: the underlying mosquitto library can make nested/reentrant
/// calls through your `Callbacks` implementation. If you use interior
/// mutability, be sure to limit the scope/duration of any locks such
/// that they do no encompass any other calls (such as attempts to
/// publish or subscribe) into mosquitto.
pub trait Callbacks {
    /// called when the connection has been acknowledged by the broker.
    /// `reason` holds the connection return code.
    /// Use `reason.is_successful` to test whether the connection was
    /// successful.
    fn on_connect(&self, _client: &mut Mosq, _reason: ConnectionStatus) {}

    /// Called when the broker has received the DISCONNECT command
    fn on_disconnect(&self, _client: &mut Mosq, _reason: ReasonCode) {}

    /// Called when the message identifier by `mid` has been sent
    /// to the broker successfully.
    fn on_publish(&self, _client: &mut Mosq, _mid: MessageId) {}

    /// Called when the broker responds to a subscription request.
    fn on_subscribe(&self, _client: &mut Mosq, _mid: MessageId, _granted_qos: &[QoS]) {}

    /// Called when a message matching a subscription is received
    /// from the broker
    fn on_message(
        &self,
        _client: &mut Mosq,
        _mid: MessageId,
        _topic: String,
        _payload: &[u8],
        _qos: QoS,
        _retain: bool,
    ) {
    }

    /// Called when the broker response to an unsubscription request
    fn on_unsubscribe(&self, _client: &mut Mosq, _mid: MessageId) {}
}

impl Callbacks for () {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QoS {
    /// This is the simplest, lowest-overhead method of sending a message. The client simply
    /// publishes the message, and there is no acknowledgement by the broker.
    AtMostOnce = 0,
    /// This method guarantees that the message will be transferred successfully to the broker.
    /// The broker sends an acknowledgement back to the sender, but in the event that that the
    /// acknowledgement is lost the sender won't realise the message has got through, so will send
    /// the message again. The client will re-send until it gets the broker's acknowledgement.
    /// This means that sending is guaranteed, although the message may reach the broker more than
    /// once.
    AtLeastOnce = 1,
    /// This is the highest level of service, in which there is a sequence of four messages between
    /// the sender and the receiver, a kind of handshake to confirm that the main message has been
    /// sent and that the acknowledgement has been received.  When the handshake has been
    /// completed, both sender and receiver are sure that the message was sent exactly once.
    ExactlyOnce = 2,
}

impl Default for QoS {
    fn default() -> QoS {
        QoS::AtMostOnce
    }
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

impl<CB: Callbacks + Send + Sync> Drop for Mosq<CB> {
    fn drop(&mut self) {
        unsafe {
            sys::mosquitto_destroy(self.m);
        }
    }
}

unsafe extern "C" fn bridge_logs(
    _m: *mut sys::mosquitto,
    _: *mut c_void,
    level: c_int,
    message: *const c_char,
) {
    use log::Level;
    let level = match level as u32 {
        libmosquitto_sys::MOSQ_LOG_NOTICE | libmosquitto_sys::MOSQ_LOG_INFO => Level::Info,
        libmosquitto_sys::MOSQ_LOG_WARNING => Level::Warn,
        libmosquitto_sys::MOSQ_LOG_ERR => Level::Error,
        libmosquitto_sys::MOSQ_LOG_DEBUG => Level::Debug,
        _ => Level::Trace,
    };
    let message = CStr::from_ptr(message).to_string_lossy();
    log::log!(level, "{message}");
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn setting_auth() {
        let mosq = Mosq::with_auto_id(()).unwrap();
        mosq.set_username_and_password(None, None).unwrap();
        mosq.set_username_and_password(Some("user"), None).unwrap();
        mosq.set_username_and_password(Some("user"), Some("pass"))
            .unwrap();
    }

    #[test]
    fn setting_some_options() {
        let mosq = Mosq::with_auto_id(()).unwrap();
        mosq.set_int_option(sys::mosq_opt_t::MOSQ_OPT_PROTOCOL_VERSION, 3)
            .unwrap();
    }
}
