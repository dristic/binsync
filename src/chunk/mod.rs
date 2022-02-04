pub mod manifest;
pub mod provider;
pub mod sync;

use serde::{Deserialize, Serialize};

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

/// Trait for providing chunks. This allows you to customize the implementation
/// of your provider to be from anywhere. For example it can be another folder
/// on the same drive or fetch chunks from the internet using any protocol.
pub trait ChunkProvider {
    fn chunk_exists(&self, key: &u64) -> bool;
    fn get_chunk(&self, key: &u64) -> Vec<u8>;
}
