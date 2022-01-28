pub mod manifest;

use std::{
    collections::HashMap,
    convert::TryInto,
    fs::{File, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use fastcdc::FastCDC;

use crate::error::Error;

use self::manifest::Manifest;

pub trait ChunkProvider {
    fn chunk_exists(&self, key: &u64) -> bool;
    fn get_chunk(&self, key: &u64) -> Vec<u8>;
}

struct ProviderChunk {
    file: PathBuf,
    offset: u64,
    length: u64,
}

pub struct BasicChunkProvider {
    chunks: HashMap<u64, ProviderChunk>,
}

impl BasicChunkProvider {
    pub fn new<P: AsRef<Path>>(path: P, manifest: &Manifest) -> BasicChunkProvider {
        let prefix = path.as_ref().to_path_buf();
        let mut chunks = HashMap::new();

        for (file_path, file_chunks) in &manifest.files {
            for chunk in file_chunks {
                chunks.insert(
                    chunk.hash,
                    ProviderChunk {
                        file: prefix.join(Path::new(&file_path)),
                        offset: chunk.offset,
                        length: chunk.length,
                    },
                );
            }
        }

        BasicChunkProvider { chunks }
    }
}

impl ChunkProvider for BasicChunkProvider {
    fn chunk_exists(&self, key: &u64) -> bool {
        self.chunks.contains_key(key)
    }

    fn get_chunk(&self, key: &u64) -> Vec<u8> {
        if !self.chunks.contains_key(key) {
            return Vec::new();
        }

        let chunk_info = self.chunks.get(key).unwrap();

        // TODO: Maintain a LRU cache of file handles.
        let mut file = File::open(&chunk_info.file).unwrap();
        let mut buffer = vec![0; chunk_info.length.try_into().unwrap()];

        file.seek(SeekFrom::Start(chunk_info.offset)).unwrap();
        file.read_exact(&mut buffer).unwrap();

        buffer
    }
}

pub struct Syncer<T: ChunkProvider> {
    destination: PathBuf,
    chunks: T,
    manifest: Manifest,
}

impl<T: ChunkProvider> Syncer<T> {
    pub fn new<P: AsRef<Path>>(destination: P, chunks: T, manifest: Manifest) -> Syncer<T> {
        Syncer {
            destination: destination.as_ref().to_path_buf(),
            chunks,
            manifest,
        }
    }

    pub fn sync(&mut self) -> Result<(), Error> {
        for (file_path, chunks) in &self.manifest.files {
            let path = self.destination.join(Path::new(&file_path));

            println!("Syncing file {:?}", path.to_str().unwrap());

            let mut source_file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(path)
                .unwrap();

            let mut contents = Vec::new();
            source_file.read_to_end(&mut contents).unwrap();
            let chunker = FastCDC::new(&contents, 16384, 32768, 65536);

            let mut have_chunks = HashMap::new();

            for entry in chunker {
                let end = entry.offset + entry.length;
                let chunk = &contents[entry.offset..end];

                let digest = md5::compute(chunk);
                let hash = u64::from_le_bytes(digest[0..8].try_into().unwrap());

                have_chunks.insert(hash, chunk);
            }

            source_file.seek(SeekFrom::Start(0)).unwrap();

            for chunk in chunks.iter() {
                if have_chunks.contains_key(&chunk.hash) {
                    source_file
                        .write_all(have_chunks.get(&chunk.hash).unwrap())
                        .unwrap();
                } else {
                    source_file
                        .write_all(&self.chunks.get_chunk(&chunk.hash))
                        .unwrap();
                }
            }

            let pos = source_file.seek(SeekFrom::Current(0)).unwrap();
            source_file.set_len(pos).unwrap();
        }

        Ok(())
    }
}
