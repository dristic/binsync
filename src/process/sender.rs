use std::{path::{Path, PathBuf}};
use walkdir::WalkDir;

use crate::{error::Error};

use super::{FileInfo, FileList, Message, Socket};

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
        loop {
            let response: Message = self.socket.receive()?;
            let file_list = Message::FileList(self.get_file_list());

            match response {
                Message::Empty => {},
                Message::Hello(version) =>
                {
                    println!("Server Hello: {}", version);

                    let hello = Message::Hello(2);
                    self.socket.send(&hello)?;
                    self.socket.send(&file_list)?;
                },
                Message::FileList(_) => {},
                Message::FileChecksums(checksums) =>
                {
                    println!("Server FileChecksums: {:?}", checksums);
                },
                Message::Shutdown =>
                {
                    break;
                }
            }
        }

        Ok(())
    }

    pub fn get_file_list(&self) -> FileList {
        let mut list = FileList{
            files: Vec::new()
        };
    
        for entry in WalkDir::new(&self.source) {
            let info = entry.unwrap();
    
            if info.file_type().is_file() {
                let name = info.file_name().to_string_lossy().to_string();
                let directory = info
                    .path()
                    .strip_prefix(&self.source)
                    .map_or_else(
                        |_| { info.path().to_string_lossy().to_string() },
                        |p| { p.to_string_lossy().to_string() }
                    );

                list.files.push(FileInfo{
                    name,
                    directory
                });
            }
        }

        list
    }
}
