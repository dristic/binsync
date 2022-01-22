use std::{
    collections::HashMap,
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use crate::{
    error::{self, Error},
    process::{FileChecksums, FileInfo, SyncMessage},
};

use super::{Message, Socket, API_VERSION, CHUNK_SIZE};
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
            .open(&path)
            .expect("Unable to open file for reading.");

        println!("File length: {}", file.metadata().unwrap().len());

        let mut checksums: HashMap<u32, [u8; 16]> = HashMap::new();
        let mut offsets: HashMap<[u8; 16], u64> = HashMap::new();
        let mut offset = 0;

        loop {
            let mut chunk: Vec<u8> = Vec::with_capacity(CHUNK_SIZE);

            let num_read = std::io::Read::by_ref(&mut file)
                .take(CHUNK_SIZE as u64)
                .read_to_end(&mut chunk)
                .map_err(|_| error::Error::new("Unable to read from file."))?;

            let mut adler = simd_adler32::Adler32::new();
            adler.write(&chunk);
            let hash = adler.finish();

            let digest = md5::compute(chunk);

            checksums.insert(hash, *digest);
            offsets.insert(*digest, offset);

            offset += CHUNK_SIZE as u64;

            if num_read < CHUNK_SIZE {
                break;
            }
        }

        self.socket
            .send(&Message::FileChecksums(FileChecksums { id, checksums }))?;

        let mut extension = path
            .extension()
            .unwrap_or_else(|| OsStr::new(""))
            .to_os_string();
        extension.push(".tmp");
        let temp_path = path.with_extension(extension);
        let mut new_file = File::create(&temp_path).expect("Unable to open file for reading.");

        file.seek(SeekFrom::Start(0))
            .map_err(|_| error::Error::new("Unable to seek"))?;

        loop {
            let response: SyncMessage = self.socket.receive()?;

            match response {
                SyncMessage::FileBytes(file_bytes) => {
                    new_file
                        .write_all(&file_bytes)
                        .map_err(|_| error::Error::new("Unable to write to file"))?;
                }
                SyncMessage::FileChecksum(checksum) => {
                    let offset = offsets[&checksum];
                    file.seek(SeekFrom::Start(offset))
                        .map_err(|_| error::Error::new("Unable to seek"))?;

                    let mut take = std::io::Read::by_ref(&mut file).take(CHUNK_SIZE as u64);
                    std::io::copy(&mut take, &mut new_file)
                        .map_err(|_| error::Error::new("Unable to copy bytes"))?;
                }
                SyncMessage::FileEnd => {
                    println!("Client FileEnd");
                    break;
                }
            }
        }

        fs::remove_file(&path).map_err(|_| error::Error::new("Unable to delete file"))?;
        fs::rename(&temp_path, &path).map_err(|_| error::Error::new("Unable to rename file"))?;

        Ok(())
    }

    pub fn close(&mut self) -> Result<(), Error> {
        let shutdown = Message::Shutdown;

        self.socket.send(&shutdown)?;

        Ok(())
    }
}
