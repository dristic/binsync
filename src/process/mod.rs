mod sender;
mod receiver;

use crate::error::Error;
use serde::{Deserialize, Serialize};

pub use sender::*;
pub use receiver::*;

pub const API_VERSION: u32 = 1;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct FileInfo {
    pub name: String,
    pub directory: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct FileList {
    pub files: Vec<FileInfo>,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct FileChecksums {
    pub id: u32,
    pub checksums: Vec<u32>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Empty,
    Hello(u32),
    FileList(FileList),
    FileChecksums(FileChecksums),
    Shutdown
}

pub trait Socket {
    fn send<T: ?Sized>(&mut self, value: &T) -> Result<(), Error>
    where
        T: serde::Serialize;

    fn receive<'a, T>(&mut self) -> Result<T, Error>
    where
        T: serde::de::DeserializeOwned;
}