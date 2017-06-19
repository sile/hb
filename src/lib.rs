extern crate fibers;
extern crate futures;
extern crate miasht;
extern crate serde;
#[macro_use]
extern crate serde_derive;
extern crate serdeconv;
#[macro_use]
extern crate trackable;
extern crate url;
extern crate url_serde;

pub use error::{Error, ErrorKind};

pub mod request;
pub mod run;

mod error;

pub type Result<T> = ::std::result::Result<T, Error>;
