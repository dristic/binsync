use std::{
    collections::HashMap,
    convert::TryInto,
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use fastcdc::FastCDC;

use crate::{error::Error, Manifest};

use super::ChunkProvider;

pub struct Syncer<T: ChunkProvider> {
    destination: PathBuf,
    provider: T,
    manifest: Manifest,
}

impl<T: ChunkProvider> Syncer<T> {
    pub fn new<P: AsRef<Path>>(destination: P, provider: T, manifest: Manifest) -> Syncer<T> {
        Syncer {
            destination: destination.as_ref().to_path_buf(),
            provider,
            manifest,
        }
    }

    pub fn sync(&mut self) -> Result<(), Error> {
        for (file_path, chunks) in &self.manifest.files {
            let path = self.destination.join(Path::new(&file_path));

            // Since this should be a file it should always have a parent.
            let parent = path
                .parent()
                .ok_or_else(|| Error::FileNotFound(path.to_path_buf()))?;
            fs::create_dir_all(&parent).map_err(|_| Error::AccessDenied)?;

            let mut source_file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(path)
                .unwrap();

            let mut contents = Vec::new();
            source_file.read_to_end(&mut contents).unwrap();
            let chunker = FastCDC::new(&contents, 16384, 32768, 65536);

            let mut have_chunks = HashMap::new();

            for entry in chunker {
                let end = entry.offset + entry.length;
                let chunk = &contents[entry.offset..end];

                let digest = md5::compute(chunk);
                let hash = u64::from_le_bytes(digest[0..8].try_into().unwrap());

                have_chunks.insert(hash, chunk);
            }

            source_file.seek(SeekFrom::Start(0)).unwrap();

            // TODO: Seek forward chunks that are already in the correct place.
            for chunk in chunks.iter() {
                if have_chunks.contains_key(&chunk.hash) {
                    source_file
                        .write_all(have_chunks.get(&chunk.hash).unwrap())
                        .unwrap();
                } else {
                    source_file
                        .write_all(&self.provider.get_chunk(&chunk.hash))
                        .unwrap();
                }
            }

            let pos = source_file.seek(SeekFrom::Current(0)).unwrap();
            source_file.set_len(pos).unwrap();
        }

        Ok(())
    }
}
