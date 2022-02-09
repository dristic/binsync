// Copyright (c) 2021 Dan Ristic

//! This crate implements functions for syncing arbitrary binary content between
//! two locations. The underlying algorithm used is based on fast content
//! defined chunking. This works by first generating a `Manifest` that lists
//! the files and chunks in the source folder. Using the `sync` function pointed
//! at an empty or already populated destination folder the libray will migrate
//! the contents of the destination to look like the source.
//!
//! ```rust,no_run
//! use binsync::{Manifest, BasicChunkProvider, Syncer};
//!
//! let manifest = Manifest::from_path("foo/source");
//!
//! let basic_provider = BasicChunkProvider::new("foo/source", &manifest);
//!
//! let mut syncer = Syncer::new("foo/destination", basic_provider, manifest);
//! syncer.sync().unwrap();
//! ```

mod chunk;
mod error;

pub use chunk::{
    manifest::Manifest,
    provider::{BasicChunkProvider, CachingChunkProvider},
    sync::Syncer,
    ChunkProvider,
};
pub use error::Error as BinsyncError;
use std::path::Path;

// Helper function to generate a manifest from the given path.
pub fn generate_manifest(from: &str) -> Result<Manifest, BinsyncError> {
    let from_path = Path::new(&from);
    if !from_path.exists() {
        return Err(BinsyncError::FileNotFound(from_path.to_path_buf()));
    }

    let manifest = Manifest::from_path(from_path);
    Ok(manifest)
}

/// Helper function to execute the syncer on a given path with a provider and
/// manifest supplied.
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

/// Helper function to sync the given input and output directories using the
/// `BasicChunkProvider`. This is inefficient but a good example to understand
/// how the library works in order to customize for your own application.
pub fn sync(from: &str, to: &str) -> Result<(), BinsyncError> {
    let manifest = generate_manifest(&from)?;

    let from_path = Path::new(&from);
    let basic_provider = CachingChunkProvider::new(from_path, &manifest);

    sync_from_manifest(manifest, basic_provider, to)
}

pub fn sync_with_progress(
    from: &str,
    to: &str,
    on_progress: impl FnMut(u32),
) -> Result<(), BinsyncError> {
    let manifest = generate_manifest(&from)?;

    let from_path = Path::new(&from);
    let basic_provider = BasicChunkProvider::new(from_path, &manifest);

    let to_path = Path::new(&to);
    let mut syncer = Syncer::new(to_path, basic_provider, manifest);
    syncer.on_progress(on_progress);
    syncer.sync()?;

    Ok(())
}
