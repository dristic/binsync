// Copyright (c) 2021 Dan Ristic

//! This crate implements functions for syncing arbitrary binary content between
//! two locations. The underlying algorithm used is based on fast content
//! defined chunking. This works by first generating a `Manifest` that lists
//! the files and chunks in the source folder. Using the `sync` function pointed
//! at an empty or already populated destination folder the libray will migrate
//! the contents of the destination to look like the source.

mod chunk;
mod error;

pub use chunk::{manifest::Manifest, provider::BasicChunkProvider, sync::Syncer, ChunkProvider};
pub use error::Error as BinsyncError;
use std::path::Path;

pub fn generate_manifest(from: &str) -> Result<Manifest, BinsyncError> {
    let from_path = Path::new(&from);
    if !from_path.exists() {
        return Err(BinsyncError::FromFileNotFound);
    }

    let manifest = Manifest::from_path(from_path);
    Ok(manifest)
}

pub fn sync_from_manifest<T: ChunkProvider>(
    manifest: Manifest,
    provider: T,
    to: &str,
) -> Result<(), BinsyncError> {
    let to_path = Path::new(&to);
    let mut syncer = Syncer::new(to_path, provider, manifest);
    syncer.sync()?;

    Ok(())
}

pub fn sync(from: &str, to: &str) -> Result<(), BinsyncError> {
    let manifest = generate_manifest(&from)?;

    let from_path = Path::new(&from);
    let basic_provider = BasicChunkProvider::new(from_path, &manifest);

    sync_from_manifest(manifest, basic_provider, to)
}
