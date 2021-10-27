use std::{
    fs::OpenOptions,
    io::Read,
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

use crate::error::{self, Error};

use super::{FileBytes, FileInfo, FileList, Message, Socket, SyncMessage};

const CHUNK_SIZE: usize = 1100;

pub struct Sender<T: Socket> {
    source: PathBuf,
    socket: T,
}

impl<T: Socket> Sender<T> {
    pub fn new<P: AsRef<Path>>(source: P, socket: T) -> Sender<T> {
        Sender {
            source: source.as_ref().to_path_buf(),
            socket,
        }
    }

    pub fn listen(&mut self) -> Result<(), Error> {
        loop {
            let response: Message = self.socket.receive()?;
            let file_list = self.get_file_list();

            match response {
                Message::Empty => {}
                Message::Hello(version) => {
                    println!("Server Hello: {}", version);

                    let hello = Message::Hello(2);
                    self.socket.send(&hello)?;
                    self.socket.send(&Message::FileList(file_list))?;
                }
                Message::FileList(_) => {}
                Message::FileChecksums(checksums) => {
                    println!("Server FileChecksums: {:?}", checksums);

                    let file = &file_list.files[checksums.id];

                    self.send_deltas(file)?;
                }
                Message::Shutdown => {
                    break;
                }
            }
        }

        Ok(())
    }

    fn get_file_list(&self) -> FileList {
        let mut list = FileList { files: Vec::new() };

        for entry in WalkDir::new(&self.source) {
            let info = entry.unwrap();

            if info.file_type().is_file() {
                let name = info.file_name().to_string_lossy().to_string();
                let directory = info.path().strip_prefix(&self.source).map_or_else(
                    |_| info.path().to_string_lossy().to_string(),
                    |p| p.to_string_lossy().to_string(),
                );

                list.files.push(FileInfo { name, directory });
            }
        }

        list
    }

    fn send_deltas(&mut self, from: &FileInfo) -> Result<(), Error> {
        let from_path = self.source.join(Path::new(&from.directory));
        let mut file = OpenOptions::new()
            .read(true)
            .open(from_path)
            .map_err(|_| error::Error::new("Unable to open file for reading."))?;

        loop {
            let mut chunk: Vec<u8> = Vec::with_capacity(CHUNK_SIZE);

            let num_read = file
                .by_ref()
                .take(CHUNK_SIZE as u64)
                .read_to_end(&mut chunk)
                .map_err(|_| error::Error::new("Unable to read from file."))?;

            self.socket
                .send(&SyncMessage::FileBytes(FileBytes { id: 0, data: chunk }))?;

            if num_read < CHUNK_SIZE {
                break;
            }
        }

        self.socket.send(&SyncMessage::FileEnd)?;

        Ok(())
    }
}
