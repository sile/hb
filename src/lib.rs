extern crate fibers;
extern crate fibers_http_client;
extern crate futures;
extern crate httpcodec;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serdeconv;
#[macro_use]
extern crate slog;
#[macro_use]
extern crate trackable;
extern crate url;
extern crate url_serde;

pub use error::{Error, ErrorKind};

pub mod request;
pub mod run;
pub mod summary;
pub mod time_series;

mod error;

pub type Result<T> = ::std::result::Result<T, Error>;
