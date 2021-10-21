use std::{path::{Path, PathBuf}};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::{error::Error};

use super::{Message, Socket};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct FileInfo {
    pub name: String,
    pub directory: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct FileList {
    pub files: Vec<FileInfo>,
}

pub struct Sender<T: Socket> {
    source: PathBuf,
    socket: T
}

impl<T: Socket> Sender<T> {
    pub fn new<P: AsRef<Path>>(source: P, socket: T) -> Sender<T> {
        Sender {
            source: source.as_ref().to_path_buf(),
            socket
        }
    }

    pub fn listen(&mut self) -> Result<(), Error> {
        let response: Message = self.socket.receive()?;

        match response {
            Message::Empty => {},
            Message::Hello(version) =>
            {
                println!("Got hello {}", version)
            }
        }

        let hello = Message::Hello(2);

        self.socket.send(&hello)?;

        Ok(())
    }

    pub fn get_file_list(&self) -> FileList {
        let mut list = FileList{
            files: Vec::new()
        };
    
        for entry in WalkDir::new(&self.source) {
            let info = entry.unwrap();
    
            if info.file_type().is_file() {
                list.files.push(FileInfo{
                    name: info.file_name().to_os_string().to_string_lossy().to_string(),
                    directory: info.path().to_string_lossy().to_string()
                });
            }
        }

        list
    }
}
