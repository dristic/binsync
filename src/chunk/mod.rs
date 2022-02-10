pub mod manifest;
pub mod provider;
pub mod sync;

use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};

/// The most basic building block. Holds the precomputed hash identifier along
/// with the offset in the file and length of the chunk.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Chunk {
    pub hash: u64,
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

/// When planning a sync instead of performing it directly this is used to
/// describe the operations needed to sync two folders together.
pub struct SyncPlan {
    /// Map of files that need to be transformed and what operations we need to
    /// perform on them.
    pub operations: HashMap<PathBuf, Vec<Operation>>,

    /// Total number of operations in the plan.
    pub total_ops: u32,
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

    /// Gets the raw data of the chunk.
    fn get_chunk(&self, key: &u64) -> Vec<u8>;
}
