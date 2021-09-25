mod sender;
mod receiver;

use crate::error::Error;

pub use sender::*;
pub use receiver::*;

pub struct Message {}

pub trait Socket<T> {
    fn send(&self, t: T) -> Result<(), Error>;
}