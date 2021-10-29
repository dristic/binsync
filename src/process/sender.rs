use std::{fs::OpenOptions, io::{BufReader, Read}, path::{Path, PathBuf}};
use adler32::RollingAdler32;
use walkdir::WalkDir;

use crate::error::{self, Error};

use super::{CHUNK_SIZE, FileChecksums, FileInfo, FileList, Message, Socket, SyncMessage};

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
                    println!("Server FileChecksums: {}", checksums.checksums.len());

                    let file = &file_list.files[checksums.id];

                    self.send_deltas(&checksums, file)?;
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

    fn send_deltas(&mut self, checksums: &FileChecksums, from: &FileInfo) -> Result<(), Error> {
        let checksums = &checksums.checksums;
        let from_path = self.source.join(Path::new(&from.directory));
        let file = OpenOptions::new()
            .read(true)
            .open(from_path)
            .map_err(|_| error::Error::new("Unable to open file for reading."))?;

        let reader = BufReader::new(file);
        let mut buffer = Vec::with_capacity(CHUNK_SIZE);
        let mut send_buffer = Vec::with_capacity(CHUNK_SIZE);
        let mut adler = RollingAdler32::new();

        for byte in reader.bytes() {
            let byte = byte
                .map_err(|_| error::Error::new("Unable to read byte"))?;

            adler.update(byte);
            buffer.push(byte);

            if buffer.len() == CHUNK_SIZE {
                let hash = adler.hash();

                if let Some(have_digest) = checksums.get(&hash) {
                    let dest_digest = md5::compute(&buffer);

                    if send_buffer.len() > 0 {
                        self.socket.send(&SyncMessage::FileBytes(send_buffer))?;
                        send_buffer = Vec::with_capacity(CHUNK_SIZE);
                    }

                    if have_digest.eq(&*dest_digest) {
                        self.socket.send(&SyncMessage::FileChecksum(*have_digest))?;

                        adler = RollingAdler32::new();
                        buffer.clear();
                    }
                } else {
                    let byte = buffer.remove(0);
                    send_buffer.push(byte);

                    if send_buffer.len() == CHUNK_SIZE {
                        self.socket.send(&SyncMessage::FileBytes(send_buffer))?;
                        send_buffer = Vec::with_capacity(CHUNK_SIZE);
                    }
                }
            }
        }

        if send_buffer.len() > 0 {
            self.socket.send(&SyncMessage::FileBytes(send_buffer))?;
        }

        self.socket.send(&SyncMessage::FileBytes(buffer))?;

        self.socket.send(&SyncMessage::FileEnd)?;

        Ok(())
    }
}
