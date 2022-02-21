use std::{
    collections::{HashMap, HashSet},
    convert::TryInto,
    sync::{
        mpsc, Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
};

use reqwest::blocking::Client;
use serde::{Deserialize, Serialize};

use crate::{BinsyncError, ChunkProvider, Manifest};

use super::{ChunkId, Operation};

/// ID type for packs defined in a single location.
type PackId = u64;

const DEFAULT_PACK_SIZE: usize = 104857600; // 100MB

/// A pack of chunks bundled together for network optimization.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct Pack {
    pub hash: PackId,
    pub length: u64,
    pub chunks: Vec<ChunkId>,
}

/// Wraps a chunk manifest so that chunks can be logically grouped into packs.
/// Packs reduce the amount of requests needed to sync across a remote pipe.
#[derive(Serialize, Deserialize, PartialEq, Debug)]
pub struct RemoteManifest {
    pub source: Manifest,
    pub packs: Vec<Pack>,
}

impl RemoteManifest {
    /// Generate a remote manifest by packing chunks from an existing manifest.
    /// This picks a `DEFAULT_PACK_SIZE` defined in code.
    pub fn from_manifest(manifest: Manifest) -> RemoteManifest {
        RemoteManifest::with_pack_size(DEFAULT_PACK_SIZE, manifest)
    }

    /// Similar to from_manifest with a custom pack size limit. Will pack chunks
    /// up to the limit without going over.
    pub fn with_pack_size(size: usize, manifest: Manifest) -> RemoteManifest {
        let mut packs = Vec::new();

        let mut length = 0;
        let mut bytes: Vec<u8> = Vec::new();
        let mut chunks: Vec<ChunkId> = Vec::new();

        for (_, file_chunks) in &manifest.files {
            for chunk in file_chunks {
                // If we do not have space save off a new pack.
                if length + chunk.length > size as u64 {
                    let digest = md5::compute(bytes);
                    let hash = u64::from_le_bytes(digest[0..8].try_into().unwrap());
                    packs.push(Pack {
                        hash,
                        length,
                        chunks,
                    });

                    length = 0;
                    bytes = Vec::new();
                    chunks = Vec::new();
                }

                length += chunk.length;
                // TODO: This is slow but gives the best collision avoidance.
                bytes.append(&mut chunk.hash.to_le_bytes().to_vec());
                chunks.push(chunk.hash);
            }
        }

        // If we still have a partial pack save it off.
        if length > 0 {
            let digest = md5::compute(bytes);
            let hash = u64::from_le_bytes(digest[0..8].try_into().unwrap());
            packs.push(Pack {
                hash,
                length,
                chunks,
            });
        }

        RemoteManifest {
            source: manifest,
            packs,
        }
    }
}

enum PackMessage {
    Download(u64),
    Terminate,
}

/// The pack downloader handles fetching pack data on a background thread. Also
/// has a memory upper bound and will download packs until the upper bound
/// is reached.
struct PackDownloader {
    thread: Option<JoinHandle<()>>,
    sender: mpsc::Sender<PackMessage>,

    /// Holds the shared memory of downloaded packs with the download thread.
    packs: Arc<Mutex<HashMap<PackId, Vec<u8>>>>,
    on_pack: Arc<Condvar>,

    /// Holds already downloaded packs in a main-thread-owned cache.
    cache: HashMap<PackId, Vec<u8>>,
    size: u64,
}

impl PackDownloader {
    pub fn with_size(size: u64, base_url: &str) -> PackDownloader {
        let mut thread = None;
        let (sender, receiver) = mpsc::channel();
        let packs = Arc::new(Mutex::new(HashMap::new()));
        let on_pack = Arc::new(Condvar::new());
        let cache = HashMap::new();
        let size = 0;

        // Setup the base url to append pack ids to.
        let mut base_url = base_url.to_string();
        if !base_url.ends_with('/') {
            base_url.push('/');
        }

        // Setup our download thread.
        {
            let packs = Arc::clone(&packs);
            let on_pack = Arc::clone(&on_pack);
            let client = Client::new();
            let handle = thread::spawn(move || loop {
                let message = receiver.recv().unwrap();

                match message {
                    PackMessage::Download(pack_id) => {
                        let url = format!("{}{}.binpack", base_url, pack_id);
                        println!("Fetching pack {}", url);
                        let request = client.get(url);

                        let response = match request.send() {
                            Ok(response) => response,
                            Err(_) => {
                                println!("Failed to send request.");
                                on_pack.notify_all();
                                break;
                            }
                        };

                        if !response.status().is_success() {
                            println!("Received failure response from URL {}", response.status());
                            on_pack.notify_all();
                            break;
                        }

                        let data = match response.bytes() {
                            Ok(data) => data,
                            Err(_) => {
                                println!("Failed to get data.");
                                on_pack.notify_all();
                                break;
                            }
                        };

                        packs.lock().unwrap().insert(pack_id, data.to_vec());
                        on_pack.notify_all();
                    }
                    PackMessage::Terminate => break,
                }
            });

            thread = Some(handle);
        }

        PackDownloader {
            thread,
            sender,
            packs,
            on_pack,
            cache,
            size,
        }
    }

    pub fn fetch_pack(&mut self, id: PackId) {
        self.sender.send(PackMessage::Download(id)).unwrap();
    }

    pub fn get_pack<'a>(&'a mut self, id: PackId) -> &'a [u8] {
        // If this is in the cache, return it.
        if self.cache.contains_key(&id) {
            return self.cache.get(&id).unwrap().as_slice();
        }

        // Download pack if it does not exist.
        let mut packs = self.packs.lock().unwrap();
        if !packs.contains_key(&id) {
            //self.sender.send(PackMessage::Download(id)).unwrap();
            while !packs.contains_key(&id) {
                packs = self.on_pack.wait(packs).unwrap();
            }
        }

        if let Some(pack) = packs.remove(&id) {
            // Move this into the cache.
            self.cache.insert(id, pack);
        }

        self.cache.get(&id).unwrap().as_slice()
    }
}

impl Drop for PackDownloader {
    fn drop(&mut self) {
        self.sender.send(PackMessage::Terminate).unwrap();

        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}

#[derive(Debug)]
struct ChunkPackInfo {
    pack_id: PackId,
    offset: u64,
    length: u64,
}

/// A simple remote chunk provider from the given URI. Will make GET network
/// requests against the URI with the chunk hash appended to the end. Will also
/// attempt to cache chunks in LRU with a given memory budget.
///
/// # WIP
///
/// This class is heavily a work in progress. It is functional but far from
/// optimal in its handling of error cases.
pub struct RemoteChunkProvider {
    downloader: PackDownloader,
    chunk_map: HashMap<ChunkId, ChunkPackInfo>,
}

const CACHE_LIMIT: u64 = 104857600; // 100MB

impl RemoteChunkProvider {
    pub fn new(base_url: &str, manifest: &RemoteManifest) -> Result<RemoteChunkProvider, BinsyncError> {
        let downloader = PackDownloader::with_size(CACHE_LIMIT, base_url);
        let mut chunk_map = HashMap::new();

        // Build a local map of chunk_id => chunk for use in the next step.
        let mut chunks = HashMap::new();
        for (_, file_chunks) in &manifest.source.files {
            for chunk in file_chunks {
                chunks.insert(chunk.hash, chunk);
            }
        }

        // Now build our list of pack information.
        for pack in &manifest.packs {
            let mut offset: u64 = 0;

            for chunk_id in &pack.chunks {
                match chunks.get(chunk_id) {
                    Some(chunk) => {
                        chunk_map.insert(chunk_id.clone(), ChunkPackInfo {
                            pack_id: pack.hash,
                            offset,
                            length: chunk.length,
                        });

                        offset += chunk.length;
                    },
                    None => return Err(BinsyncError::ChunkNotFound(chunk_id.clone())),
                }
            }
        }

        Ok(RemoteChunkProvider {
            downloader,
            chunk_map,
        })
    }
}

impl ChunkProvider for RemoteChunkProvider {
    fn set_plan(&mut self, plan: &super::SyncPlan) {
        let mut packs_to_cache = HashSet::new();

        // Figure out all the packs we might need.
        for (_, operations) in &plan.operations {
            for operation in operations {
                // If we need to fetch this from the remote source.
                if let Operation::Fetch(chunk) = operation {
                    if let Some(pack_info) = self.chunk_map.get(&chunk.hash) {
                        packs_to_cache.insert(pack_info.pack_id);
                    }
                }
            }
        }

        // Tell the downloader to pre-fetch them all, deciding when the
        // internal cache is full to stop.
        for pack_id in packs_to_cache {
            self.downloader.fetch_pack(pack_id);
        }
    }

    fn get_chunk<'a>(&'a mut self, key: &u64) -> Result<&'a [u8], BinsyncError> {
        let pack_info = self
            .chunk_map
            .get(key)
            .ok_or_else(|| BinsyncError::ChunkNotFound(key.clone()))?;

        let data = self.downloader.get_pack(pack_info.pack_id);

        let start = pack_info.offset as usize;
        let end = (pack_info.offset + pack_info.length) as usize;
        if data.len() < end {
            println!("Length mismatch {:?} {} {}", pack_info, data.len(), end);
            return Err(BinsyncError::Unspecified);
        }

        Ok(&data[start..end])
    }
}
