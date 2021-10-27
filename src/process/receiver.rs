use std::{
    fs::OpenOptions,
    io::{Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use crate::{
    error::{self, Error},
    process::{FileChecksums, FileInfo, SyncMessage},
};

use super::{Message, Socket, API_VERSION};
pub struct Receiver<T: Socket> {
    destination: PathBuf,
    socket: T,
}

impl<T: Socket> Receiver<T> {
    pub fn new<P: AsRef<Path>>(destination: P, socket: T) -> Receiver<T> {
        Receiver {
            destination: destination.as_ref().to_path_buf(),
            socket,
        }
    }

    pub fn sync(&mut self) -> Result<(), Error> {
        let hello = Message::Hello(API_VERSION);
        self.socket.send(&hello)?;

        loop {
            let response: Message = self.socket.receive()?;

            match response {
                Message::Empty => {}
                Message::Hello(version) => {
                    println!("Client Hello: {}", version);
                }
                Message::FileList(list) => {
                    println!("Client FileList: {:?}", list);

                    for (i, file_info) in list.files.iter().enumerate() {
                        self.sync_file(i as usize, file_info)?;
                    }

                    break;
                }
                Message::FileChecksums(_) => {}
                Message::Shutdown => {
                    break;
                }
            }
        }

        Ok(())
    }

    pub fn sync_file(&mut self, id: usize, file_info: &FileInfo) -> Result<(), Error> {
        let path = self.destination.join(Path::new(&file_info.directory));
        let mut file = OpenOptions::new()
            .write(true)
            .read(true)
            .create(true)
            .open(path)
            .expect("Unable to open file for reading.");

        println!("File length: {}", file.metadata().unwrap().len());

        let checksums = vec![];

        self.socket
            .send(&Message::FileChecksums(FileChecksums { id, checksums }))?;

        file.seek(SeekFrom::Start(0))
            .map_err(|_| error::Error::new("Unable to seek file"))?;

        loop {
            let response: SyncMessage = self.socket.receive()?;

            match response {
                SyncMessage::FileBytes(file_bytes) => {
                    file.write_all(&file_bytes.data)
                        .map_err(|_| error::Error::new("Unable to write to file"))?;
                }
                SyncMessage::FileEnd => {
                    println!("Client FileEnd");
                    break;
                }
            }
        }

        Ok(())
    }

    pub fn close(&mut self) -> Result<(), Error> {
        let shutdown = Message::Shutdown;

        self.socket.send(&shutdown)?;

        Ok(())
    }
}
