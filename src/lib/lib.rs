#![allow(clippy::needless_return)]
#![warn(clippy::implicit_return)]

#[macro_use]
extern crate lazy_static;
#[macro_use]
extern crate log;

mod error;
mod old;
mod spawn;
pub use error::Error;
pub use old::*;
