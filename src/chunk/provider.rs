use std::{
    collections::HashMap,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
};

use super::{ChunkId, ChunkProvider, Operation, SyncPlan};

use crate::BinsyncError;

struct ProviderChunk {
    file: PathBuf,
    offset: u64,
    length: u64,
    ref_count: u32,
    data: Option<Vec<u8>>,
}

/// A caching chunk provider for local transfers. Attempts to read and save
/// chunks as optimally as possible by caching file handles and chunks that
/// are used more than once.
pub struct CachingChunkProvider {
    source: PathBuf,
    chunks: HashMap<ChunkId, ProviderChunk>,
    empty_chunk: Option<ChunkId>,
}

impl CachingChunkProvider {
    pub fn new<P: AsRef<Path>>(path: P) -> CachingChunkProvider {
        CachingChunkProvider {
            source: PathBuf::from(path.as_ref()),
            chunks: HashMap::new(),
            empty_chunk: None,
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
                                    data: None,
                                },
                            );
                        }
                    }
                }
            }
        }
    }

    fn get_chunk<'a>(&'a mut self, key: &u64) -> Result<&'a [u8], BinsyncError> {
        if let Some(chunk_id) = self.empty_chunk {
            self.chunks.remove(&chunk_id);
        }

        if let Some(chunk) = self.chunks.get_mut(key) {
            chunk.ref_count = chunk.ref_count - 1;

            // If this is no longer needed set it for deletion.
            if chunk.ref_count == 0 {
                self.empty_chunk = Some(key.clone());
            }

            // First check the cache.
            if let None = chunk.data {
                // Not in the cache so lets read it.
                let mut file = File::open(&chunk.file).map_err(|_| BinsyncError::AccessDenied)?;
                let mut buffer = vec![0; chunk.length as usize];

                file.seek(SeekFrom::Start(chunk.offset))
                    .map_err(|_| BinsyncError::AccessDenied)?;
                file.read_exact(&mut buffer)
                    .map_err(|_| BinsyncError::AccessDenied)?;

                chunk.data = Some(buffer);
            }

            // It will either be cached or just fetched.
            if let Some(data) = &chunk.data {
                return Ok(&data[..]);
            }
        }

        // Not sure why this is requesting a chunk not in the plan.
        Err(BinsyncError::ChunkNotFound(key.clone()))
    }
}
