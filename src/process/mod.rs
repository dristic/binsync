mod receiver;
mod sender;

use std::collections::HashMap;

use crate::error::Error;
use serde::{Deserialize, Serialize};

pub use receiver::*;
pub use sender::*;

pub const API_VERSION: u32 = 1;

const CHUNK_SIZE: usize = 1100;

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
    pub id: usize,
    pub checksums: HashMap<u32, [u8; 16]>,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum Message {
    Empty,
    Hello(u32),
    FileList(FileList),
    FileChecksums(FileChecksums),
    Shutdown,
}

#[derive(Serialize, Deserialize, Debug)]
pub enum SyncMessage {
    FileBytes(Vec<u8>),
    FileChecksum([u8; 16]),
    FileEnd,
}

pub trait Socket {
    fn send<T: ?Sized>(&mut self, value: &T) -> Result<(), Error>
    where
        T: serde::Serialize;

    fn receive<'a, T>(&mut self) -> Result<T, Error>
    where
        T: serde::de::DeserializeOwned;
}
