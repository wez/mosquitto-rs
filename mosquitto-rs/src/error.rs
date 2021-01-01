use crate::lowlevel::sys::mosq_err_t;
use std::collections::HashMap;
use std::os::raw::c_int;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("creation error: {0}")]
    Create(std::io::Error),
    #[error("c-string mapping error")]
    CString(#[from] std::ffi::NulError),
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),
    #[error("mosq error: {:?}", .0)]
    Mosq(mosq_err_t),
    #[error("mosq error code {0}")]
    UnknownMosq(c_int),
    #[error("hostname resolution error: {0}")]
    Resolution(String),
}

lazy_static::lazy_static! {
    static ref ERRMAP: HashMap<c_int, mosq_err_t> = Error::build_map();
}

impl Error {
    fn build_map() -> HashMap<c_int, mosq_err_t> {
        let mut map = HashMap::new();
        macro_rules! m {
            ($($a:ident),* $(,)?) => {
                $(
                    map.insert(mosq_err_t::$a as c_int, mosq_err_t::$a);
                 )*
            };
        }
        m!(
            MOSQ_ERR_AUTH_CONTINUE,
            MOSQ_ERR_NO_SUBSCRIBERS,
            MOSQ_ERR_SUB_EXISTS,
            MOSQ_ERR_CONN_PENDING,
            MOSQ_ERR_SUCCESS,
            MOSQ_ERR_NOMEM,
            MOSQ_ERR_PROTOCOL,
            MOSQ_ERR_INVAL,
            MOSQ_ERR_NO_CONN,
            MOSQ_ERR_CONN_REFUSED,
            MOSQ_ERR_NOT_FOUND,
            MOSQ_ERR_CONN_LOST,
            MOSQ_ERR_TLS,
            MOSQ_ERR_PAYLOAD_SIZE,
            MOSQ_ERR_NOT_SUPPORTED,
            MOSQ_ERR_AUTH,
            MOSQ_ERR_ACL_DENIED,
            MOSQ_ERR_UNKNOWN,
            MOSQ_ERR_ERRNO,
            MOSQ_ERR_EAI,
            MOSQ_ERR_PROXY,
            MOSQ_ERR_PLUGIN_DEFER,
            MOSQ_ERR_MALFORMED_UTF8,
            MOSQ_ERR_KEEPALIVE,
            MOSQ_ERR_LOOKUP,
            MOSQ_ERR_MALFORMED_PACKET,
            MOSQ_ERR_DUPLICATE_PROPERTY,
            MOSQ_ERR_TLS_HANDSHAKE,
            MOSQ_ERR_QOS_NOT_SUPPORTED,
            MOSQ_ERR_OVERSIZE_PACKET,
            MOSQ_ERR_OCSP,
        );

        map
    }

    pub(crate) fn result<T>(err: c_int, res: T) -> Result<T, Self> {
        if err == mosq_err_t::MOSQ_ERR_SUCCESS as c_int {
            Ok(res)
        } else {
            Err(Self::from_err(err))
        }
    }

    pub(crate) fn from_err(err: c_int) -> Self {
        if err == mosq_err_t::MOSQ_ERR_ERRNO as c_int {
            Self::IO(std::io::Error::last_os_error())
        } else if err == mosq_err_t::MOSQ_ERR_EAI as c_int {
            // Mosquitto stuffs the getaddrinfo() error code into errno,
            // so we can extract it and get the message manually here
            unsafe {
                let err = std::io::Error::last_os_error();
                let err = err.raw_os_error().unwrap_or(0);
                let reason = std::ffi::CStr::from_ptr(libc::gai_strerror(err));
                Self::Resolution(reason.to_string_lossy().into())
            }
        } else {
            if let Some(e) = ERRMAP.get(&err) {
                Self::Mosq(*e)
            } else {
                Self::UnknownMosq(err)
            }
        }
    }
}
