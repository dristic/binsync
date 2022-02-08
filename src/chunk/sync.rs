use std::{
    collections::HashMap,
    convert::TryInto,
    fs::{self, OpenOptions},
    io::{Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use fastcdc::FastCDC;

use crate::{error::Error, Manifest};

use super::{manifest::Chunk, ChunkProvider};

/// When planning a sync instead of performing it directly this is used to
/// describe the operations needed to sync two folders together.
pub struct SyncPlan {
    pub operations: HashMap<PathBuf, Vec<Operation>>,
}

/// A single operation in a sync plan.
pub enum Operation {
    Seek(i64), // Since seek can go both ways it uses a signed int.
    Copy(Chunk),
    Fetch(u64),
}

/// Uses a manifest and a provider to sync data to the destination.
pub struct Syncer<'a, T: ChunkProvider> {
    destination: PathBuf,
    provider: T,
    manifest: Manifest,
    progress: Option<Box<dyn FnMut(u32) + 'a>>,
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

    /// Sets a function to receive progress updates. Every time a file is
    /// completed this is fired with a number from 0 percent to 100.
    pub fn on_progress(&mut self, f: impl FnMut(u32) + 'a) {
        self.progress = Some(Box::new(f));
    }

    /// Plans an update with the current `Manifest` and settings. Returns a plan
    /// of what files should update with a list of operations for each file.
    pub fn plan(&self) -> Result<SyncPlan, Error> {
        let mut plan = SyncPlan {
            operations: HashMap::new(),
        };

        for (file_path, chunks) in &self.manifest.files {
            let mut operations = Vec::new();
            let path = self.destination.join(Path::new(&file_path));

            let mut have_chunks = HashMap::new();

            // If we have an existing file extract chunks from it.
            if path.exists() {
                let mut source_file = OpenOptions::new()
                    .read(true)
                    .open(&path)
                    .map_err(|_| Error::AccessDenied)?;

                // TODO: We read the entire file to memory. Instead we should
                // be able to do this in subsections based on a max memory limit.
                let mut contents = Vec::new();
                source_file
                    .read_to_end(&mut contents)
                    .map_err(|_| Error::AccessDenied)?;
                let chunker = FastCDC::new(&contents, 16384, 32768, 65536);

                for entry in chunker {
                    let end = entry.offset + entry.length;
                    let data = &contents[entry.offset..end];

                    let digest = md5::compute(data);
                    let hash = u64::from_le_bytes(digest[0..8].try_into().unwrap());

                    have_chunks.insert(hash, entry);
                }
            }

            for chunk in chunks.iter() {
                match have_chunks.get(&chunk.hash) {
                    Some(entry) => {
                        if entry == chunk {
                            // The chunk is already in the right place.
                            let seek_len: i64 = entry.length as i64;
                            operations.push(Operation::Seek(seek_len));
                        } else {
                            // We have the same chunk, but elsewhere in the file.
                            operations.push(Operation::Copy(Chunk {
                                hash: chunk.hash,
                                offset: entry.offset as u64,
                                length: entry.length as u64,
                            }));
                        }
                    }
                    None => {
                        // We need to get this chunk from our provider.
                        operations.push(Operation::Fetch(chunk.hash));
                    }
                }
            }

            plan.operations.insert(path.to_path_buf(), operations);
        }

        Ok(plan)
    }

    /// Exectues a sync from source to destination with the current parameters.
    pub fn sync(&mut self) -> Result<(), Error> {
        let mut num_files: u32 = 0;
        let total: u32 = self
            .manifest
            .files
            .len()
            .try_into()
            .map_err(|_| Error::Unspecified)?;

        let sync_plan = self.plan()?;

        for (file_path, operations) in sync_plan.operations {
            // Since this should be a file it should always have a parent.
            let parent = file_path
                .parent()
                .ok_or_else(|| Error::FileNotFound(file_path.to_path_buf()))?;
            fs::create_dir_all(&parent).map_err(|_| Error::AccessDenied)?;

            let mut source_file = OpenOptions::new()
                .read(true)
                .write(true)
                .create(true)
                .open(file_path)
                .map_err(|_| Error::AccessDenied)?;

            let mut have_chunks = HashMap::new();

            // First load all the chunk copies into memory.
            for operation in &operations {
                if let Operation::Copy(chunk) = operation {
                    source_file
                        .seek(SeekFrom::Start(chunk.offset))
                        .map_err(|_| Error::AccessDenied)?;

                    let mut data = vec![0; chunk.length as usize];
                    source_file
                        .read_exact(&mut data)
                        .map_err(|_| Error::AccessDenied)?;

                    have_chunks.insert(chunk.hash, data);
                }
            }

            source_file
                .seek(SeekFrom::Start(0))
                .map_err(|_| Error::AccessDenied)?;

            // Now operate!
            for operation in &operations {
                match operation {
                    Operation::Seek(len) => {
                        source_file
                            .seek(SeekFrom::Current(*len))
                            .map_err(|_| Error::AccessDenied)?;
                    }
                    Operation::Copy(chunk) => {
                        let data = have_chunks
                            .get(&chunk.hash)
                            .ok_or_else(|| Error::Unspecified)?;

                        source_file
                            .write_all(data)
                            .map_err(|_| Error::AccessDenied)?;
                    }
                    Operation::Fetch(hash) => {
                        source_file
                            .write_all(&self.provider.get_chunk(hash))
                            .map_err(|_| Error::AccessDenied)?;
                    }
                }
            }

            // Truncate the file to the correct length.
            let pos = source_file
                .seek(SeekFrom::Current(0))
                .map_err(|_| Error::AccessDenied)?;
            source_file.set_len(pos).map_err(|_| Error::AccessDenied)?;

            // Update our percentage after this file.
            num_files = num_files + 1;

            if let Some(f) = &mut self.progress {
                let percent: u32 = (num_files / total) * 100;
                (*f)(percent);
            }
        }

        Ok(())
    }
}
