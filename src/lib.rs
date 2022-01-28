mod chunk;
mod error;
mod net;
mod process;

pub use chunk::{manifest::Manifest, BasicChunkProvider, Syncer};
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

pub fn sync(from: &str, to: &str) -> Result<(), BinsyncError> {
    let manifest = generate_manifest(&from)?;

    let from_path = Path::new(&from);
    let basic_provider = BasicChunkProvider::new(from_path, &manifest);

    let to_path = Path::new(&to);
    let mut syncer = Syncer::new(to_path, basic_provider, manifest);
    syncer.sync()?;

    Ok(())
}
