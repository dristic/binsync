pub mod manifest;
pub mod provider;
pub mod sync;

#[cfg(feature = "network")]
pub mod network;

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::BinsyncError;

/// Constant values for the CDC chunker. The producer and consumer need to use
/// the same values so be careful changing these.
const MIN_CHUNK: usize = 32768;
const AVG_CHUNK: usize = 65536;
const MAX_CHUNK: usize = 131072;

/// ChunkId is defined for the entire crate in a single location.
type ChunkId = u64;

/// The most basic building block. Holds the precomputed hash identifier along
/// with the offset in the file and length of the chunk.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Chunk {
    pub hash: ChunkId,
    pub offset: u64,
    pub length: u64,
}

impl PartialEq<fastcdc::Chunk> for Chunk {
    fn eq(&self, other: &fastcdc::Chunk) -> bool {
        self.offset == other.offset as u64 && self.length == other.length as u64
    }
}

impl PartialEq<Chunk> for fastcdc::Chunk {
    fn eq(&self, other: &Chunk) -> bool {
        self.offset == other.offset as usize && self.length == other.length as usize
    }
}

/// Name and directory of a single file.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct FileInfo {
    pub name: String,
    pub directory: String,
}

/// List of files from a given root.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct FileList {
    pub files: Vec<FileInfo>,
}

/// This describes the operations we need to take in order to transform the
/// source into the destination. All operations are performed in-order but
/// may be multi-threaded to speed up syncing.
pub struct SyncPlan {
    /// Map of files that need to be transformed and what operations we need to
    /// perform on them.
    pub operations: Vec<(PathBuf, Vec<Operation>)>,

    /// Total number of operations in the plan.
    pub total_ops: u32,
}

impl SyncPlan {
    /// Gets the size of all the fetch operations for this plan.
    pub fn get_fetch_size(&self) -> u64 {
        let mut size: u64 = 0;

        for (_, operations) in &self.operations {
            for operation in operations {
                if let Operation::Fetch(chunk) = operation {
                    size = size + chunk.length;
                }
            }
        }

        size
    }
}

/// A single operation in a sync plan.
pub enum Operation {
    Seek(i64), // Since seek can go both ways it uses a signed int.
    Copy(Chunk),
    Fetch(Chunk),
}

/// Trait for providing chunks. This allows you to customize the implementation
/// of your provider to be from anywhere. For example it can be another folder
/// on the same drive or fetch chunks from the internet using any protocol.
pub trait ChunkProvider {
    /// Sets the plan for the provider when it is ready. This allows the
    /// provider to make decisions on how it wants to optimize chunk reading.
    fn set_plan(&mut self, plan: &SyncPlan);

    /// Gets the raw data of the chunk. The provider may choose to modify its
    /// internal cache when fetching a chunk.
    fn get_chunk<'a>(&'a mut self, key: &u64) -> Result<&'a [u8], BinsyncError>;
}
