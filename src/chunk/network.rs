use std::{
    collections::HashMap,
    convert::TryInto,
    sync::{
        mpsc::{self, Receiver},
        Arc,
    },
};

use serde::{Deserialize, Serialize};

use crate::{sync::ThreadPool, BinsyncError, ChunkProvider, Manifest};

use super::ChunkId;

/// ID type for packs defined in a single location.
type PackId = u64;

const DEFAULT_PACK_SIZE: usize = 4194304; // 4MB

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

/// Runs download operations on a background thread.
struct AsyncDownloader {
    pool: ThreadPool,
    base_url: String,
    client: Arc<reqwest::blocking::Client>,
}

impl AsyncDownloader {
    pub fn new(base_url: &str) -> AsyncDownloader {
        let pool = ThreadPool::new(1);

        // Setup the base url to append pack ids to.
        let mut base_url = base_url.to_string();
        if !base_url.ends_with('/') {
            base_url.push('/');
        }

        let client = reqwest::blocking::Client::new();

        AsyncDownloader {
            pool,
            base_url,
            client: Arc::new(client),
        }
    }

    pub fn download_pack(&self, pack_id: PackId) -> Receiver<Option<Vec<u8>>> {
        let (sender, receiver) = mpsc::channel();
        let url = format!("{}{}.binpack", self.base_url, pack_id);
        let client = Arc::clone(&self.client);

        self.pool.execute(move || {
            println!("Fetching pack {}", url);
            let request = client.get(url);

            let response = match request.send() {
                Ok(response) => response,
                Err(_) => {
                    println!("Failed to send request.");
                    sender.send(None).unwrap();
                    return;
                }
            };

            if !response.status().is_success() {
                println!("Received failure response from URL {}", response.status());
                sender.send(None).unwrap();
                return;
            }

            match response.bytes() {
                Ok(data) => sender.send(Some(data.to_vec())).unwrap(),
                Err(_) => {
                    println!("Failed to get data.");
                    sender.send(None).unwrap();
                    return;
                }
            };
        });

        receiver
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
/// attempt to cache chunks.
///
/// # WIP
///
/// This class is heavily a work in progress. It is functional but far from
/// optimal.
pub struct RemoteChunkProvider {
    chunk_cache: HashMap<ChunkId, Vec<u8>>,
    downloader: AsyncDownloader,
    chunk_map: HashMap<ChunkId, ChunkPackInfo>,
}

impl RemoteChunkProvider {
    pub fn new(
        base_url: &str,
        manifest: &RemoteManifest,
    ) -> Result<RemoteChunkProvider, BinsyncError> {
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
            chunk_cache: HashMap::new(),
            downloader: AsyncDownloader::new(base_url),
            chunk_map,
        })
    }
}

impl ChunkProvider for RemoteChunkProvider {
    fn set_plan(&mut self, _plan: &super::SyncPlan) {
        // TODO: Start fetching content, reference count chunks
    }

    fn get_chunk<'a>(&'a mut self, key: &u64) -> Result<&'a [u8], BinsyncError> {
        // If we already have it, return it.
        if self.chunk_cache.contains_key(&key) {
            return Ok(self.chunk_cache.get(&key).unwrap().as_slice());
        }

        // If not, download the pack and cache the chunks.
        let pack = self.chunk_map.get(&key);

        if pack.is_none() {
            return Err(BinsyncError::Unspecified(String::from("Pack not found!")));
        }

        let pack = pack.unwrap();
        match self.downloader.download_pack(pack.pack_id).recv().unwrap() {
            Some(data) => {
                if data.len() != pack.pack_length as usize {
                    return Err(BinsyncError::Unspecified(String::from(
                        "Pack length does not match",
                    )));
                }

                // Cache all the chunks from this pack
                for chunk in &self.chunk_map {
                    let chunk_info = chunk.1;
                    if chunk_info.pack_id == pack.pack_id {
                        let start = chunk_info.offset as usize;
                        let end = (chunk_info.offset + chunk_info.length) as usize;

                        if data.len() >= end {
                            self.chunk_cache
                                .insert(chunk.0.clone(), data[start..end].to_vec());
                        }
                    }
                }
            }
            None => {
                // Something went wrong
                return Err(BinsyncError::Unspecified(String::from(
                    "Got none downloading pack",
                )));
            }
        }

        if let Some(chunk) = self.chunk_cache.get(key) {
            Ok(chunk.as_slice())
        } else {
            // Something went wrong
            Err(BinsyncError::Unspecified(String::from(
                "Could not find chunk after download",
            )))
        }
    }
}
