use std::{
    collections::HashMap,
    convert::TryInto,
    fs::OpenOptions,
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use fastcdc::FastCDC;
use walkdir::WalkDir;

use crate::{
    error::Error,
    process::{FileInfo, FileList},
};

pub trait ChunkMap {
    fn chunk_exists(&self, key: &u64) -> bool;
    fn get_chunk(&mut self, key: &u64) -> Vec<u8>;
    fn get_chunks(&mut self, name: &str) -> Vec<u64>;
}

pub struct Syncer<T: ChunkMap> {
    destination: PathBuf,
    chunks: T,
    file_list: FileList,
}

impl<T: ChunkMap> Syncer<T> {
    pub fn new<P: AsRef<Path>>(destination: P, chunks: T, file_list: FileList) -> Syncer<T> {
        Syncer {
            destination: destination.as_ref().to_path_buf(),
            chunks,
            file_list,
        }
    }

    pub fn sync(&mut self) -> Result<(), Error> {
        for file_info in &self.file_list.files {
            let path = self.destination.join(Path::new(&file_info.directory));

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

            let file_chunks = self.chunks.get_chunks(file_info.name.as_str());

            let mut have_chunks = HashMap::new();

            for entry in chunker {
                let end = entry.offset + entry.length;
                let chunk = &contents[entry.offset..end];

                let digest = md5::compute(chunk);
                let hash = u64::from_le_bytes(digest[0..8].try_into().unwrap());

                have_chunks.insert(hash, chunk);
            }

            source_file.seek(SeekFrom::Start(0)).unwrap();

            for hash in file_chunks.iter() {
                if have_chunks.contains_key(&hash) {
                    source_file
                        .write_all(have_chunks.get(hash).unwrap())
                        .unwrap();
                } else {
                    source_file.write_all(&self.chunks.get_chunk(hash)).unwrap();
                }
            }

            let pos = source_file.seek(SeekFrom::Current(0)).unwrap();
            source_file.set_len(pos).unwrap();
        }

        Ok(())
    }
}

pub struct Generator {
    source: PathBuf,
    chunks: HashMap<u64, Vec<u8>>,
    manifest: HashMap<String, Vec<u64>>,
}

impl Generator {
    pub fn new<P: AsRef<Path>>(source: P) -> Generator {
        Generator {
            source: source.as_ref().to_path_buf(),
            chunks: HashMap::new(),
            manifest: HashMap::new(),
        }
    }

    pub fn generate(&mut self, file_list: &FileList) -> Result<(), Error> {
        for file_info in &file_list.files {
            let path = self.source.join(Path::new(&file_info.directory));

            println!("Generating for file {:?}", path.to_str().unwrap());

            let contents = std::fs::read(path).unwrap();
            let chunker = FastCDC::new(&contents, 16384, 32768, 65536);

            let mut file_chunks = Vec::new();

            for entry in chunker {
                let end = entry.offset + entry.length;
                let chunk = &contents[entry.offset..end];

                let digest = md5::compute(chunk);
                let hash = u64::from_le_bytes(digest[0..8].try_into().unwrap());

                self.chunks.insert(hash, chunk.to_vec());

                file_chunks.push(hash);
            }

            self.manifest.insert(file_info.name.clone(), file_chunks);
        }

        Ok(())
    }

    pub fn get_file_list(&self) -> FileList {
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
}

impl ChunkMap for Generator {
    fn chunk_exists(&self, key: &u64) -> bool {
        self.chunks.contains_key(key)
    }

    fn get_chunk(&mut self, key: &u64) -> Vec<u8> {
        self.chunks.get(key).unwrap().to_vec()
    }

    fn get_chunks(&mut self, name: &str) -> Vec<u64> {
        self.manifest.get(name).unwrap().to_vec()
    }
}
