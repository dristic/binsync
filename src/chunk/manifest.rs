use std::{collections::HashMap, convert::TryInto, path::Path};

use fastcdc::FastCDC;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::chunk::{FileInfo, FileList};

/// The most basic building block. Holds the precomputed hash identifier along
/// with the offset in the file and length of the chunk.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Chunk {
    pub hash: u64,
    pub offset: u64,
    pub length: u64,
}

/// Holds a list of files and which chunks exist inside those files in which
/// order. The manifest is the source of the syncer allowing us to know what
/// we should be syncing to.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Manifest {
    pub files: HashMap<String, Vec<Chunk>>,
}

impl Manifest {
    pub fn new() -> Manifest {
        Manifest {
            files: HashMap::new(),
        }
    }

    /// Generates a manifest using the specified path as the root.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Manifest {
        let mut list = FileList { files: Vec::new() };

        let prefix = path.as_ref().to_path_buf();

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
        let mut manifest = Manifest::new();
        let prefix = path.as_ref().to_path_buf();

        for file_info in &file_list.files {
            let path = prefix.join(Path::new(&file_info.directory));

            let contents = std::fs::read(path).unwrap();
            let chunker = FastCDC::new(&contents, 16384, 32768, 65536);

            let key = file_info.directory.as_str();

            for entry in chunker {
                let end = entry.offset + entry.length;
                let chunk = &contents[entry.offset..end];

                let digest = md5::compute(chunk);
                let hash = u64::from_le_bytes(digest[0..8].try_into().unwrap());

                manifest.add_chunk(
                    key,
                    hash,
                    entry.offset.try_into().unwrap(),
                    entry.length.try_into().unwrap(),
                );
            }
        }

        manifest
    }

    fn add_chunk(&mut self, path: &str, hash: u64, offset: u64, length: u64) {
        let key = path.to_string();
        if !self.files.contains_key(&key) {
            self.files.insert(key, Vec::new());
        }

        self.files.get_mut(&path.to_string()).unwrap().push(Chunk {
            hash,
            offset,
            length,
        });
    }
}
