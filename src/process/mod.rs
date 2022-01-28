mod receiver;
mod sender;

use std::{collections::HashMap, path::Path, thread};

use crate::{error::Error as BinsyncError, net::client::LocalSocketClient};
use interprocess::local_socket::LocalSocketListener;
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
    fn send<T: ?Sized>(&mut self, value: &T) -> Result<(), BinsyncError>
    where
        T: serde::Serialize;

    fn receive<'a, T>(&mut self) -> Result<T, BinsyncError>
    where
        T: serde::de::DeserializeOwned;
}

pub fn _sync(from: &str, to: &str) -> Result<(), Box<dyn std::error::Error>> {
    let to_path = Path::new(to);

    // Validate options.
    let from_path = Path::new(from);
    if !from_path.exists() {
        return Err(Box::new(BinsyncError::new("Cannot find from file.")));
    }

    // Negotiate protocol (future)

    // Establish connection
    let listener = LocalSocketListener::bind("/tmp/binsync.sock")?;
    let client = LocalSocketClient::connect()?;
    let host = LocalSocketClient {
        stream: listener.accept()?,
    };

    let mut sender = Sender::new(&from_path, host);
    let mut receiver = Receiver::new(&to_path, client);

    // Initiate the transfer.
    let sender_thread = thread::spawn(move || -> Result<(), BinsyncError> {
        sender
            .listen()
            .map_err(|_| BinsyncError::new("Failed to listen."))
    });

    receiver.sync()?;
    receiver.close()?;

    sender_thread
        .join()
        .unwrap()
        .map_err(|_| BinsyncError::new("Thread join failed."))?;

    Ok(())
}
