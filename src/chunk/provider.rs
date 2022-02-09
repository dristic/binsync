use std::{
    collections::HashMap,
    convert::TryInto,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use crate::Manifest;

use super::ChunkProvider;

struct ProviderChunk {
    file: PathBuf,
    offset: u64,
    length: u64,
}

/// Helper function that takes a manifest and base path and converts it to a
/// hash map based on chunk hashes.
fn convert_manifest_to_map<P: AsRef<Path>>(
    path: P,
    manifest: &Manifest,
) -> HashMap<u64, ProviderChunk> {
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

    chunks
}

/// The most basic chunk provider for syncing. Performs no caching and simply
/// reads chunk off disk when a chunk is requested. Use this either as an
/// example or if the complexity of other chunk providers is not compatible
/// with your application.
pub struct BasicChunkProvider {
    chunks: HashMap<u64, ProviderChunk>,
}

impl BasicChunkProvider {
    pub fn new<P: AsRef<Path>>(path: P, manifest: &Manifest) -> BasicChunkProvider {
        let chunks = convert_manifest_to_map(path, manifest);
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

        let mut file = File::open(&chunk_info.file).unwrap();
        let mut buffer = vec![0; chunk_info.length.try_into().unwrap()];

        file.seek(SeekFrom::Start(chunk_info.offset)).unwrap();
        file.read_exact(&mut buffer).unwrap();

        buffer
    }
}

/// A memory caching chunk provider. This will read chunks into memory on a
/// separate thread and cache them in memory. It will also reference count the
/// chunks based on the manifest passed in so it does not read the same chunk
/// more than once. This is a great deal more efficient than the basic provider.
pub struct CachingChunkProvider {
    chunks: HashMap<u64, ProviderChunk>,
}

impl ChunkProvider for CachingChunkProvider {
    fn chunk_exists(&self, key: &u64) -> bool {
        self.chunks.contains_key(key)
    }

    fn get_chunk(&self, _key: &u64) -> Vec<u8> {
        todo!()
    }
}

/// A simple remote chunk provider from the given URI. Will make GET network
/// requests against the URI with the chunk hash appended to the end. Will also
/// attempt to cache chunks in LRU with a given memory budget.
pub struct RemoteChunkProvider {
    chunks: HashMap<u64, ProviderChunk>,
}

impl ChunkProvider for RemoteChunkProvider {
    fn chunk_exists(&self, key: &u64) -> bool {
        self.chunks.contains_key(key)
    }

    fn get_chunk(&self, _key: &u64) -> Vec<u8> {
        todo!()
    }
}
