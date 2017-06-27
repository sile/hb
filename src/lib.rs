extern crate fibers;
extern crate futures;
extern crate handy_async;
extern crate miasht;
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

mod connection_pool;
mod error;

pub type Result<T> = ::std::result::Result<T, Error>;
