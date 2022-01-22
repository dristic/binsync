use adler32::RollingAdler32;
use std::{
    fs::OpenOptions,
    io::{BufReader, Read},
    path::{Path, PathBuf},
};
use walkdir::WalkDir;

use crate::error::{self, Error};

use super::{FileChecksums, FileInfo, FileList, Message, Socket, SyncMessage, CHUNK_SIZE};

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
        let file_list = self.get_file_list();

        loop {
            let response: Message = self.socket.receive()?;

            match response {
                Message::Empty => {}
                Message::Hello(version) => {
                    println!("Server Hello: {}", version);

                    let hello = Message::Hello(2);
                    self.socket.send(&hello)?;
                    self.socket.send(&Message::FileList(self.get_file_list()))?;
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
            let byte = byte.map_err(|_| error::Error::new("Unable to read byte"))?;

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

#[cfg(test)]
mod tests {
    use std::{
        sync::{atomic::AtomicU32, Arc},
        thread,
    };

    use super::*;

    struct FakeSocket {
        pub recvs: Arc<AtomicU32>,
        messages: Vec<Vec<u8>>,
    }

    impl Socket for FakeSocket {
        fn send<T: ?Sized>(&mut self, _: &T) -> Result<(), Error>
        where
            T: serde::Serialize,
        {
            Ok(())
        }

        fn receive<'a, T>(&mut self) -> Result<T, Error>
        where
            T: serde::de::DeserializeOwned,
        {
            let next = self.messages.pop().unwrap();

            self.recvs.fetch_add(1, std::sync::atomic::Ordering::SeqCst);

            bincode::deserialize(&next).map_err(|_| error::Error::new("Failed to deserialize."))
        }
    }

    #[test]
    fn can_listen() {
        let recvs = Arc::new(AtomicU32::new(0));

        let source = Path::new(".");
        let socket = FakeSocket {
            recvs: recvs.clone(),
            messages: vec![bincode::serialize(&Message::Shutdown).unwrap()],
        };

        let mut sender = Sender::new(source, socket);
        let thread = thread::spawn(move || {
            sender.listen().unwrap();
        });

        thread.join().unwrap();

        assert_eq!(recvs.load(std::sync::atomic::Ordering::SeqCst), 1);
    }
}
