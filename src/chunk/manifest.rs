use std::{
    convert::TryInto,
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
};

use fastcdc::FastCDC;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::{
    chunk::{FileInfo, FileList},
    sync::ThreadPool,
};

use super::{Chunk, AVG_CHUNK, MAX_CHUNK, MIN_CHUNK};

/// Information about a file and which chunks it contains.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct FileChunkInfo {
    pub path: PathBuf,
    pub chunks: Vec<Chunk>,
}

/// Holds a list of files and which chunks exist inside those files in which
/// order. The manifest is the source of the syncer allowing us to know what
/// we should be syncing to.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Manifest {
    pub files: Vec<FileChunkInfo>,
}

impl Manifest {
    pub fn new() -> Manifest {
        Manifest { files: Vec::new() }
    }

    /// Generates a manifest using the specified path as the root.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Manifest {
        let mut list = FileList { files: Vec::new() };

        let prefix = path.as_ref().to_path_buf();

        // TODO: Handle empty folders.

        for entry in WalkDir::new(&prefix) {
            let info = entry.unwrap();

            if info.file_type().is_file() {
                let name = info.file_name().to_string_lossy().to_string();
                let directory = info.path().strip_prefix(&prefix).map_or_else(
                    |_| info.path().to_string_lossy().to_string(),
                    |p| p.to_string_lossy().to_string(),
                );

                list.files.push(FileInfo { name, directory });
            }
        }

        Manifest::from_file_list(path, &list)
    }

    /// Generates a manifest of specific files using the specified path as the
    /// base path. Use this if you want to filter only to specific files in the
    /// directory.
    pub fn from_file_list<P: AsRef<Path>>(path: P, file_list: &FileList) -> Manifest {
        let manifest = Arc::new(Mutex::new(Manifest::new()));
        let prefix = path.as_ref().to_path_buf();

        let pool = ThreadPool::new(4);

        for file_info in &file_list.files {
            let key = file_info.directory.clone();
            let path = prefix.join(Path::new(&file_info.directory));
            let manifest = Arc::clone(&manifest);

            pool.execute(move || {
                let contents = std::fs::read(path).unwrap();
                let chunker = FastCDC::new(&contents, MIN_CHUNK, AVG_CHUNK, MAX_CHUNK);

                let mut file_chunk_info = FileChunkInfo {
                    path: PathBuf::from(key),
                    chunks: Vec::new(),
                };

                for entry in chunker {
                    let end = entry.offset + entry.length;
                    let chunk = &contents[entry.offset..end];

                    let digest = md5::compute(chunk);
                    let hash = u64::from_le_bytes(digest[0..8].try_into().unwrap());

                    file_chunk_info.chunks.push(Chunk {
                        hash,
                        offset: entry.offset as u64,
                        length: entry.length as u64,
                    });
                }

                manifest.lock().unwrap().files.push(file_chunk_info);
            });
        }

        drop(pool);

        let mut manifest = Arc::try_unwrap(manifest).unwrap().into_inner().unwrap();

        manifest.files.sort_by_cached_key(|k| k.path.clone());

        manifest
    }
}

impl Default for Manifest {
    fn default() -> Self {
        Self::new()
    }
}
