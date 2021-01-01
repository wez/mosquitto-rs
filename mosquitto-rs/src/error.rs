use thiserror::Error;
use std::os::raw::c_int;

#[derive(Error, Debug)]
pub enum Error {
    #[error("creation error")]
    Create(#[from] std::io::Error),
    #[error("c-string mapping error")]
    CString(#[from] std::ffi::NulError),
}

impl Error {
}
