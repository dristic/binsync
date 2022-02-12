use std::{
    collections::HashMap,
    convert::TryInto,
    fs::{self, OpenOptions},
    io::{BufWriter, Read, Seek, SeekFrom, Write},
    path::{Path, PathBuf},
};

use fastcdc::FastCDC;

use crate::{error::Error, Manifest};

use super::{Chunk, ChunkProvider, Operation, SyncPlan};

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
            total_ops: 0,
        };

        let mut total_ops = 0;

        // TODO: We could parallelize this per-file or per-slice to get better
        // usage of the CPU cores and always be utilizing disk I/O
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
                        operations.push(Operation::Fetch(Chunk {
                            hash: chunk.hash,
                            offset: chunk.offset,
                            length: chunk.length,
                        }));
                    }
                }

                total_ops = total_ops + 1;
            }

            plan.operations
                .insert(Path::new(&file_path).to_path_buf(), operations);
            plan.total_ops = total_ops;
        }

        Ok(plan)
    }

    /// Exectues a sync from source to destination with the current parameters.
    pub fn sync(&mut self) -> Result<(), Error> {
        let mut ops_completed: u32 = 0;

        let sync_plan = self.plan()?;

        self.provider.set_plan(&sync_plan);

        for (file_path, operations) in sync_plan.operations {
            let path = self.destination.join(file_path);

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

            let mut writer = BufWriter::new(&source_file);

            // Now operate!
            for operation in &operations {
                match operation {
                    Operation::Seek(len) => {
                        writer
                            .seek(SeekFrom::Current(*len))
                            .map_err(|_| Error::AccessDenied)?;
                    }
                    Operation::Copy(chunk) => {
                        let data = have_chunks
                            .get(&chunk.hash)
                            .ok_or_else(|| Error::Unspecified)?;

                        writer.write_all(data).map_err(|_| Error::AccessDenied)?;
                    }
                    Operation::Fetch(chunk) => {
                        let data = self.provider.get_chunk(&chunk.hash)?;
                        writer.write_all(&data).map_err(|_| Error::AccessDenied)?;
                    }
                }

                ops_completed = ops_completed + 1;

                // Update our progress
                if let Some(f) = &mut self.progress {
                    let percent = (ops_completed as f32 / sync_plan.total_ops as f32) * 100.0;
                    (*f)(percent as u32);
                }
            }

            // Truncate the file to the correct length.
            let pos = writer
                .seek(SeekFrom::Current(0))
                .map_err(|_| Error::AccessDenied)?;
            source_file.set_len(pos).map_err(|_| Error::AccessDenied)?;
        }

        Ok(())
    }
}
