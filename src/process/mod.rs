mod sender;
mod receiver;

use crate::error::Error;
use serde::{Deserialize, Serialize};

pub use sender::*;
pub use receiver::*;

pub const API_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Empty,
    Hello(u32)
}

pub trait Socket {
    fn send<T: ?Sized>(&mut self, value: &T) -> Result<(), Error>
    where
        T: serde::Serialize;

    fn receive<'a, T>(&mut self) -> Result<T, Error>
    where
        T: serde::de::DeserializeOwned;
}