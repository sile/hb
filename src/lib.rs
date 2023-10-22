#[macro_use]
extern crate trackable;

pub use error::{Error, ErrorKind};

pub mod request;
pub mod run;
pub mod summary;
pub mod time_series;

mod error;

pub type Result<T> = ::std::result::Result<T, Error>;
