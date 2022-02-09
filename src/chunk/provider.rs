use std::{
    collections::HashMap,
    convert::TryInto,
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
    sync::{Arc, Condvar, Mutex},
    thread::{self, JoinHandle},
};

use crate::Manifest;

use super::ChunkProvider;

struct ProviderChunk {
    file: PathBuf,
    offset: u64,
    length: u64,
}

/// Helper function that takes a manifest and base path and converts it to a
/// hash map based on chunk hashes.
fn convert_manifest_to_map<P: AsRef<Path>>(
    path: P,
    manifest: &Manifest,
) -> HashMap<u64, ProviderChunk> {
    let prefix = path.as_ref().to_path_buf();
    let mut chunks = HashMap::new();

    for (file_path, file_chunks) in &manifest.files {
        for chunk in file_chunks {
            chunks.insert(
                chunk.hash,
                ProviderChunk {
                    file: prefix.join(Path::new(&file_path)),
                    offset: chunk.offset,
                    length: chunk.length,
                },
            );
        }
    }

    chunks
}

/// The most basic chunk provider for syncing. Performs no caching and simply
/// reads chunk off disk when a chunk is requested. Use this either as an
/// example or if the complexity of other chunk providers is not compatible
/// with your application.
pub struct BasicChunkProvider {
    chunks: HashMap<u64, ProviderChunk>,
}

impl BasicChunkProvider {
    pub fn new<P: AsRef<Path>>(path: P, manifest: &Manifest) -> BasicChunkProvider {
        let chunks = convert_manifest_to_map(path, manifest);
        BasicChunkProvider { chunks }
    }
}

impl ChunkProvider for BasicChunkProvider {
    fn chunk_exists(&self, key: &u64) -> bool {
        self.chunks.contains_key(key)
    }

    fn get_chunk(&self, key: &u64) -> Vec<u8> {
        if !self.chunks.contains_key(key) {
            return Vec::new();
        }

        let chunk_info = self.chunks.get(key).unwrap();

        let mut file = File::open(&chunk_info.file).unwrap();
        let mut buffer = vec![0; chunk_info.length.try_into().unwrap()];

        file.seek(SeekFrom::Start(chunk_info.offset)).unwrap();
        file.read_exact(&mut buffer).unwrap();

        buffer
    }
}

/// A memory caching chunk provider. This will read chunks into memory on a
/// separate thread and cache them in memory. It will also reference count the
/// chunks based on the manifest passed in so it does not read the same chunk
/// more than once. This is a great deal more efficient than the basic provider.
pub struct CachingChunkProvider {
    chunks: Arc<Mutex<HashMap<u64, Vec<u8>>>>,
    jobs: Arc<Mutex<HashMap<u64, Arc<(Mutex<bool>, Condvar)>>>>,
    handle: Option<JoinHandle<()>>,
}

impl CachingChunkProvider {
    pub fn new<P: AsRef<Path>>(path: P, manifest: &Manifest) -> CachingChunkProvider {
        let to_fetch = convert_manifest_to_map(path, manifest);

        // Convert the manifest into a list of jobs to be fulfilled.
        let mut jobs = HashMap::new();
        for key in to_fetch.keys() {
            jobs.insert(key.clone(), Arc::new((Mutex::new(false), Condvar::new())));
        }

        let chunks = Arc::new(Mutex::new(HashMap::new()));
        let jobs = Arc::new(Mutex::new(jobs));

        let data = Arc::clone(&chunks);
        let data_jobs = Arc::clone(&jobs);

        // Spawn a thread that will run through the chunk list and start copying
        // chunk data into memory.
        let handle = thread::spawn(move || {
            for (hash, chunk) in to_fetch {
                let mut file = File::open(&chunk.file).unwrap();
                let mut buffer = vec![0; chunk.length.try_into().unwrap()];

                file.seek(SeekFrom::Start(chunk.offset)).unwrap();
                file.read_exact(&mut buffer).unwrap();

                // Insert the data into the hash map and notify any waiting
                // consumers that the data is now available.
                // TODO: Multiple arc hash maps, one with arc data, is a little
                // complex. We could replace all this with a single hash map
                // of structs that hold all the relevant information.
                data.lock().unwrap().insert(hash, buffer);
                if let Some(pair) = data_jobs.lock().unwrap().remove(&hash) {
                    let mut result = pair.0.lock().unwrap();
                    *result = true;
                    pair.1.notify_all();
                }
            }

            // TODO: Provide a way to early exit if the program needs to stop.
        });

        CachingChunkProvider {
            chunks,
            jobs,
            handle: Some(handle),
        }
    }
}

impl ChunkProvider for CachingChunkProvider {
    fn chunk_exists(&self, key: &u64) -> bool {
        self.chunks.lock().unwrap().contains_key(key)
    }

    fn get_chunk(&self, key: &u64) -> Vec<u8> {
        // First read the map and see if the data is already in the cache.
        let chunks = self.chunks.lock().unwrap();
        if let Some(data) = chunks.get(key) {
            return data.to_owned();
        }
        drop(chunks);

        // Since we did not find the data, we need to wait for the data to
        // become available.
        let pair = {
            let mut jobs = self.jobs.lock().unwrap();
            match jobs.entry(key.clone()) {
                std::collections::hash_map::Entry::Occupied(entry) => Some(Arc::clone(&entry.get().clone())),
                std::collections::hash_map::Entry::Vacant(_) => None,
            }
        };

        if let Some(pair) = pair {
            let lock = pair.0.lock().unwrap();
            let result = pair.1.wait(lock).unwrap();

            if *result == false {
                return Vec::new();
            }

            // Now that the data is available, return it.
            match self.chunks.lock().unwrap().get(key) {
                Some(data) => return data.to_owned(),
                None => return Vec::new(),
            }
        }

        // We did not find the data nor any pending job to get the data.
        Vec::new()
    }
}

impl Drop for CachingChunkProvider {
    fn drop(&mut self) {
        if let Some(handle) = self.handle.take() {
            handle.join().unwrap();
        }
    }
}

/// A simple remote chunk provider from the given URI. Will make GET network
/// requests against the URI with the chunk hash appended to the end. Will also
/// attempt to cache chunks in LRU with a given memory budget.
pub struct RemoteChunkProvider {
    chunks: HashMap<u64, ProviderChunk>,
}

impl ChunkProvider for RemoteChunkProvider {
    fn chunk_exists(&self, key: &u64) -> bool {
        self.chunks.contains_key(key)
    }

    fn get_chunk(&self, _key: &u64) -> Vec<u8> {
        todo!()
    }
}
