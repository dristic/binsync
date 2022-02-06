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

/// Uses a manifest and a provider to sync data to the destination.
pub struct Syncer<'a, T: ChunkProvider> {
    destination: PathBuf,
    provider: T,
    manifest: Manifest,
    progress: Option<Box<dyn FnMut(u32) + 'a>>
}

impl<'a, T: ChunkProvider> Syncer<'a, T> {
    pub fn new<P: AsRef<Path>>(destination: P, provider: T, manifest: Manifest) -> Syncer<'a, T> {
        Syncer {
            destination: destination.as_ref().to_path_buf(),
            provider,
            manifest,
            progress: None,
        }
    }

    pub fn on_progress(&mut self, f: impl FnMut(u32) + 'a) {
        self.progress = Some(Box::new(f));
    }

    pub fn sync(&mut self) -> Result<(), Error> {
        let mut num_files: u32 = 0;
        let total: u32 = self.manifest.files.len().try_into().map_err(|_| Error::Unspecified)?;

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
                .map_err(|_| Error::AccessDenied)?;

            // TODO: For now we keep the contents in memory.
            let mut contents = Vec::new();
            source_file
                .read_to_end(&mut contents)
                .map_err(|_| Error::AccessDenied)?;
            let chunker = FastCDC::new(&contents, 16384, 32768, 65536);

            let mut have_chunks = HashMap::new();

            for entry in chunker {
                let end = entry.offset + entry.length;
                let data = &contents[entry.offset..end];

                let digest = md5::compute(data);
                let hash = u64::from_le_bytes(digest[0..8].try_into().unwrap());

                have_chunks.insert(hash, (entry, data));
            }

            source_file
                .seek(SeekFrom::Start(0))
                .map_err(|_| Error::AccessDenied)?;

            for chunk in chunks.iter() {
                match have_chunks.get(&chunk.hash) {
                    Some((entry, data)) => {
                        // Determine if the chunk is already in the correct place.
                        if entry == chunk {
                            let seek_len: i64 =
                                entry.length.try_into().map_err(|_| Error::Unspecified)?;
                            source_file
                                .seek(SeekFrom::Current(seek_len))
                                .map_err(|_| Error::AccessDenied)?;
                        } else {
                            source_file
                                .write_all(data)
                                .map_err(|_| Error::AccessDenied)?;
                        }
                    }
                    None => {
                        // We need to get this chunk from our provider.
                        source_file
                            .write_all(&self.provider.get_chunk(&chunk.hash))
                            .map_err(|_| Error::AccessDenied)?;
                    }
                }
            }

            let pos = source_file
                .seek(SeekFrom::Current(0))
                .map_err(|_| Error::AccessDenied)?;
            source_file.set_len(pos).map_err(|_| Error::AccessDenied)?;

            num_files = num_files + 1;

            if let Some(f) = &mut self.progress {
                let percent: u32 = (num_files / total) * 100;
                (*f)(percent);
            }
        }

        Ok(())
    }
}
