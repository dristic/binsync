use std::{io::{BufReader, Read, Write}, path::{Path, PathBuf}};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

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

    pub fn listen(&self) {
        for connection in self.socket.incoming() {
            let conn = match connection {
                Ok(connection) => {
                    connection
                },
                Err(e) => {
                    eprintln!("Incoming connection failed: {}", e);
                    return;
                }
            };
            
            println!("Incoming connection!");

            // Read our length-prefix.
            let mut conn = BufReader::new(conn);
            let mut len_buf = [0 as u8; 4];
            conn.read_exact(&mut len_buf).unwrap();

            let len = i32::from_be_bytes(len_buf);
            println!("Got length {}", len);
            if len > 100000 {
                panic!("Prefix length too long {:?}", len);
            }

            // Read and decode our value.
            let mut block_buf = vec![0 as u8; len as usize];
            conn.read_exact(&mut block_buf).unwrap();

            let decoded: Message = bincode::deserialize(&block_buf[..]).unwrap();

            println!("Client anwered: {:?}", decoded);

            break;
        }
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
