use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    rc::Rc,
};

use super::{ChunkProvider, Operation, SyncPlan};

use crate::BinsyncError;

struct ProviderChunk {
    file: PathBuf,
    offset: u64,
    length: u64,
    ref_count: u32,
}

/// A caching chunk provider for local transfers. Attempts to read and save
/// chunks as optimally as possible by caching file handles and chunks that
/// are used more than once.
pub struct CachingChunkProvider {
    source: PathBuf,
    chunks: HashMap<u64, ProviderChunk>,
    cache: HashMap<u64, Rc<Vec<u8>>>,
}

impl CachingChunkProvider {
    pub fn new<P: AsRef<Path>>(path: P) -> CachingChunkProvider {
        CachingChunkProvider {
            source: PathBuf::from(path.as_ref()),
            chunks: HashMap::new(),
            cache: HashMap::new(),
        }
    }
}

impl ChunkProvider for CachingChunkProvider {
    fn set_plan(&mut self, plan: &SyncPlan) {
        for (file_path, operations) in &plan.operations {
            for operation in operations {
                if let Operation::Fetch(chunk) = operation {
                    match self.chunks.get_mut(&chunk.hash) {
                        Some(provider_chunk) => {
                            provider_chunk.ref_count = provider_chunk.ref_count + 1;
                        }
                        None => {
                            self.chunks.insert(
                                chunk.hash,
                                ProviderChunk {
                                    file: self.source.join(Path::new(&file_path)),
                                    offset: chunk.offset,
                                    length: chunk.length,
                                    ref_count: 1,
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    fn get_chunk(&mut self, key: &u64) -> Result<Rc<Vec<u8>>, BinsyncError> {
        if let Some(chunk_info) = self.chunks.get_mut(key) {
            chunk_info.ref_count = chunk_info.ref_count - 1;

            // First check the cache.
            if let Some(data) = self.cache.get(key) {
                let data = Rc::clone(data);

                if chunk_info.ref_count == 0 {
                    self.cache.remove(key);
                }

                return Ok(data);
            }

            // Not in the cache so lets read it.
            let mut file = File::open(&chunk_info.file).map_err(|_| BinsyncError::AccessDenied)?;
            let mut buffer = vec![0; chunk_info.length as usize];

            file.seek(SeekFrom::Start(chunk_info.offset))
                .map_err(|_| BinsyncError::AccessDenied)?;
            file.read_exact(&mut buffer)
                .map_err(|_| BinsyncError::AccessDenied)?;

            let data = Rc::new(buffer);

            if chunk_info.ref_count != 0 {
                self.cache.insert(key.clone(), Rc::clone(&data));
            }

            return Ok(data);
        }

        // Not sure why this is requesting a chunk not in the plan.
        Err(BinsyncError::ChunkNotFound(key.clone()))
    }
}

/// A simple remote chunk provider from the given URI. Will make GET network
/// requests against the URI with the chunk hash appended to the end. Will also
/// attempt to cache chunks in LRU with a given memory budget.
pub struct RemoteChunkProvider {}

impl ChunkProvider for RemoteChunkProvider {
    fn set_plan(&mut self, _plan: &super::SyncPlan) {
        todo!()
    }

    fn get_chunk(&mut self, _key: &u64) -> Result<Rc<Vec<u8>>, BinsyncError> {
        todo!()
    }
}
