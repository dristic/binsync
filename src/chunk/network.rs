use std::{
    collections::{HashMap, HashSet, VecDeque},
    convert::TryInto,
    sync::{
        mpsc::{self, Receiver},
        Arc, Condvar, Mutex, RwLock,
    },
    thread::{self, JoinHandle},
};

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

        for file_chunk_info in &manifest.files {
            for chunk in &file_chunk_info.chunks {
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

                // Add this chunk to the current pack hash bytes and chunk
                // offset map.
                bytes.append(&mut chunk.hash.to_le_bytes().to_vec());
                chunks.push(chunk.hash);

                // Increment our offset.
                length += chunk.length;
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
    Preload(PackId),
    Download(PackId),
    Terminate,
}

/// Inner pattern of the downloading network thread.
struct PackDownloaderInner {
    /// Holds the shared memory of downloaded packs.
    packs: VecDeque<(PackId, Vec<u8>)>,
    base_url: String,
    client: reqwest::blocking::Client,
    queue: VecDeque<PackId>,
}

impl PackDownloaderInner {
    pub fn operate(&mut self, receiver: &Receiver<PackMessage>) -> bool {
        // If the queue of work is empty just wait for the next message.
        if self.queue.len() == 0 {
            let message = receiver.recv().unwrap();
            match message {
                PackMessage::Preload(pack_id) => self.queue_pack(&pack_id),
                PackMessage::Download(pack_id) => self.queue_pack(&pack_id),
                PackMessage::Terminate => return false,
            }
        }

        // Drain the message buffer so we can prioritize the steps in the
        // correct order.
        while let Ok(message) = receiver.try_recv() {
            match message {
                PackMessage::Preload(pack_id) => self.queue_pack(&pack_id),
                PackMessage::Download(pack_id) => self.queue_pack(&pack_id),
                PackMessage::Terminate => return false,
            }
        }

        // Perform the next operation in the queue.
        if let Some(pack_id) = self.queue.pop_front() {
            let url = format!("{}{}.binpack", self.base_url, pack_id);
            println!("Fetching pack {}", url);
            let request = self.client.get(url);

            let response = match request.send() {
                Ok(response) => response,
                Err(_) => {
                    println!("Failed to send request.");
                    return false;
                }
            };

            if !response.status().is_success() {
                println!("Received failure response from URL {}", response.status());
                return false;
            }

            let data = match response.bytes() {
                Ok(data) => data,
                Err(_) => {
                    println!("Failed to get data.");
                    return false;
                }
            };

            self.packs.push_back((pack_id, data.to_vec()));
        }

        true
    }

    fn queue_pack(&mut self, pack_id: &PackId) {
        self.queue.push_back(pack_id.clone());
    }
}

/// The pack downloader handles fetching pack data on a background thread. Also
/// has a memory upper bound and will download packs until the upper bound
/// is reached.
struct PackDownloader {
    thread: Option<JoinHandle<()>>,
    sender: mpsc::Sender<PackMessage>,

    /// Holds function and state for inner background download thread.
    inner: Arc<(RwLock<PackDownloaderInner>, Condvar)>,

    /// Holds already downloaded packs in a main-thread-owned cache.
    cache: HashMap<PackId, Vec<u8>>,
    cache_limit: u64,
    cache_size: u64,
    queued_size: u64,
}

impl PackDownloader {
    pub fn with_size(size: u64, base_url: &str) -> PackDownloader {
        let (sender, receiver) = mpsc::channel();
        let cache = HashMap::new();

        // Setup the base url to append pack ids to.
        let mut base_url = base_url.to_string();
        if !base_url.ends_with('/') {
            base_url.push('/');
        }

        let inner = Arc::new((
            Mutex::new(PackDownloaderInner {
                packs: VecDeque::new(),
                base_url,
                client: reqwest::blocking::Client::new(),
                queue: VecDeque::new(),
            }),
            Condvar::new(),
        ));

        // Setup our download thread.
        let context = Arc::clone(&inner);
        let thread = Some(thread::spawn(move || loop {
            // If the queue of work is empty just wait for the next message.
            if self.queue.len() == 0 {
                let message = receiver.recv().unwrap();
                match message {
                    PackMessage::Preload(pack_id) => self.queue_pack(&pack_id),
                    PackMessage::Download(pack_id) => self.queue_pack(&pack_id),
                    PackMessage::Terminate => return false,
                }
            }

            // Drain the message buffer so we can prioritize the steps in the
            // correct order.
            while let Ok(message) = receiver.try_recv() {
                match message {
                    PackMessage::Preload(pack_id) => self.queue_pack(&pack_id),
                    PackMessage::Download(pack_id) => self.queue_pack(&pack_id),
                    PackMessage::Terminate => return false,
                }
            }

            // TODO: We cannot lock the inner here.
            if !context.0.lock().unwrap().operate(&receiver) {
                context.1.notify_all();
                break;
            }

            context.1.notify_all();
        }));

        PackDownloader {
            thread,
            sender,
            inner,
            cache,
            cache_limit: size,
            cache_size: 0,
            queued_size: 0,
        }
    }

    /// The size of the outstanding and in-memory cache in bytes.
    pub fn cache_size(&self) -> u64 {
        self.cache_size + self.queued_size
    }

    /// Tells the background thread to start downloading pack data but does
    /// not block.
    pub fn prefetch_pack(&mut self, id: PackId, length: u64) {
        if !self.cache.contains_key(&id) {
            if self.cache_size() + length < self.cache_limit {
                self.queued_size += length;

                self.sender.send(PackMessage::Preload(id)).unwrap();
            }
        }
    }

    /// Gets pack data forcing the background thread to download it if it
    /// does not exist.
    pub fn get_pack_blocking<'a>(&'a mut self, id: PackId) -> &'a [u8] {
        // If this is in the cache, return it.
        if self.cache.contains_key(&id) {
            return self.cache.get(&id).unwrap().as_slice();
        }

        // Tell the download thread we need this pack and dequeue operations.
        self.sender.send(PackMessage::Download(id)).unwrap();

        println!("Sent download message for {}", id);

        let mut inner = self.inner.0.lock().unwrap();
        while !self.cache.contains_key(&id) {
            println!("Waiting for pack {}", id);
            // Unlock the thread context and wait for an operation to complete.
            inner = self.inner.1.wait(inner).unwrap();

            // Move pack data onto the main thread so we can own it.
            while let Some((pack_id, pack_data)) = inner.packs.pop_front() {
                println!("Got pack {}", pack_id);
                self.queued_size -= pack_data.len() as u64;
                self.cache_size += pack_data.len() as u64;

                self.cache.insert(pack_id, pack_data);
            }
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
    pack_length: u64,
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
    pub fn new(
        base_url: &str,
        manifest: &RemoteManifest,
    ) -> Result<RemoteChunkProvider, BinsyncError> {
        let downloader = PackDownloader::with_size(CACHE_LIMIT, base_url);
        let mut chunk_map = HashMap::new();

        // Build a local map of chunk_id => chunk for use in the next step.
        let mut chunks = HashMap::new();
        for file_chunk_info in &manifest.source.files {
            for chunk in &file_chunk_info.chunks {
                chunks.insert(chunk.hash, chunk);
            }
        }

        // Now build our list of pack information.
        for pack in &manifest.packs {
            let mut offset: u64 = 0;

            for chunk_id in &pack.chunks {
                match chunks.get(chunk_id) {
                    Some(chunk) => {
                        chunk_map.insert(
                            chunk_id.clone(),
                            ChunkPackInfo {
                                pack_id: pack.hash,
                                pack_length: pack.length,
                                offset,
                                length: chunk.length,
                            },
                        );

                        offset += chunk.length;
                    }
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
                        packs_to_cache.insert((pack_info.pack_id, pack_info.pack_length));
                    }
                }
            }
        }

        // Tell the downloader to pre-fetch them all, deciding when the
        // internal cache is full to stop.
        for (pack_id, pack_length) in packs_to_cache {
            self.downloader.prefetch_pack(pack_id, pack_length);
        }
    }

    fn get_chunk<'a>(&'a mut self, key: &u64) -> Result<&'a [u8], BinsyncError> {
        let chunk_info = self
            .chunk_map
            .get(key)
            .ok_or_else(|| BinsyncError::ChunkNotFound(key.clone()))?;

        println!("Get chunk pack {}", chunk_info.pack_id);
        let data = self.downloader.get_pack_blocking(chunk_info.pack_id);

        let start = chunk_info.offset as usize;
        let end = (chunk_info.offset + chunk_info.length) as usize;
        if data.len() < end {
            println!("Length mismatch {:?} {} {}", chunk_info, data.len(), end);
            return Err(BinsyncError::Unspecified);
        }

        Ok(&data[start..end])
    }
}
