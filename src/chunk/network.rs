use std::{
    collections::HashMap,
    rc::Rc,
    sync::{
        atomic::{AtomicU64, Ordering},
        mpsc, Arc, Condvar, Mutex,
    },
    thread::{self, JoinHandle},
};

use reqwest::blocking::Client;

use crate::{BinsyncError, ChunkProvider};

use super::Operation;

struct RemoteChunk {
    length: u64,
    ref_count: u32,
    queued: bool,
    data: Option<Vec<u8>>,
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
    thread: Option<JoinHandle<()>>,
    sender: mpsc::Sender<ChunkMessage>,
    chunks: Arc<Mutex<HashMap<u64, RemoteChunk>>>,
    notifier: Arc<Condvar>,
    total_data: u64,
    total_failures: Arc<AtomicU64>,
}

enum ChunkMessage {
    Download(u64),
    Terminate,
}

const CACHE_LIMIT: u64 = 104857600; // 100MB

impl RemoteChunkProvider {
    pub fn new(base_url: &str) -> RemoteChunkProvider {
        let chunks: Arc<Mutex<HashMap<u64, RemoteChunk>>> = Arc::new(Mutex::new(HashMap::new()));
        let notifier = Arc::new(Condvar::new());
        let total_failures = Arc::new(AtomicU64::new(0));

        let (sender, receiver) = mpsc::channel();

        let mut base_url = base_url.to_string();

        if !base_url.ends_with('/') {
            base_url.push('/');
        }

        let client = Client::new();
        let chunk_data = Arc::clone(&chunks);
        let failure_count = Arc::clone(&total_failures);
        let notify_new = Arc::clone(&notifier);
        let thread = thread::spawn(move || loop {
            let message = receiver.recv().unwrap();

            match message {
                ChunkMessage::Download(chunk_id) => {
                    let url = format!("{}{}.chunk", base_url, chunk_id);
                    println!("Fetching chunk {}", url);
                    let request = client.get(url);

                    let response = match request.send() {
                        Ok(response) => response,
                        Err(_) => {
                            println!("Failed to send request.");
                            failure_count.fetch_add(1, Ordering::SeqCst);
                            notify_new.notify_all();
                            break;
                        }
                    };

                    if !response.status().is_success() {
                        println!("Received failure response from URL {}", response.status());
                        failure_count.fetch_add(1, Ordering::SeqCst);
                        notify_new.notify_all();
                        break;
                    }

                    let data = match response.bytes() {
                        Ok(data) => data,
                        Err(_) => {
                            println!("Failed to get data.");
                            failure_count.fetch_add(1, Ordering::SeqCst);
                            notify_new.notify_all();
                            break;
                        }
                    };

                    if let Some(chunk_info) = chunk_data.lock().unwrap().get_mut(&chunk_id) {
                        chunk_info.data = Some(data.to_vec());
                    }

                    notify_new.notify_all();
                }
                ChunkMessage::Terminate => {
                    break;
                }
            }
        });

        RemoteChunkProvider {
            thread: Some(thread),
            sender,
            chunks,
            notifier,
            total_data: 0,
            total_failures,
        }
    }
}

impl ChunkProvider for RemoteChunkProvider {
    fn set_plan(&mut self, plan: &super::SyncPlan) {
        for (_, operations) in &plan.operations {
            for operation in operations {
                // If we need to fetch this from the remote source.
                if let Operation::Fetch(chunk) = operation {
                    match self.chunks.lock().unwrap().entry(chunk.hash) {
                        // If it exists simply incremrent the ref count.
                        std::collections::hash_map::Entry::Occupied(entry) => {
                            entry.into_mut().ref_count += 1;
                        }
                        // Otherwise add a new entry.
                        std::collections::hash_map::Entry::Vacant(entry) => {
                            let mut queued = false;

                            // Decide if we should queue this download.
                            if self.total_data < CACHE_LIMIT {
                                queued = true;
                                self.total_data = self.total_data + chunk.length;

                                self.sender
                                    .send(ChunkMessage::Download(chunk.hash))
                                    .unwrap();
                            }

                            entry.insert(RemoteChunk {
                                length: chunk.length,
                                ref_count: 1,
                                queued,
                                data: None,
                            });
                        }
                    }
                }
            }
        }
    }

    fn get_chunk(&mut self, key: &u64) -> Result<Rc<Vec<u8>>, BinsyncError> {
        let mut chunks = self.chunks.lock().unwrap();

        if self.total_failures.load(Ordering::SeqCst) > 0 {
            // TODO: Properly handle network failures with retries.
            return Err(BinsyncError::Unspecified);
        }

        if !chunks.contains_key(key) {
            return Err(BinsyncError::ChunkNotFound(key.clone()));
        }

        // Get a chunk containing data, or download one and wait.
        let chunk_info = {
            let mut chunk = chunks.get_mut(key).unwrap();

            if !chunk.queued {
                self.sender
                    .send(ChunkMessage::Download(key.clone()))
                    .unwrap();
                chunk.queued = true;
            }

            while chunk.data.is_none() {
                chunks = self.notifier.wait(chunks).unwrap();
                chunk = chunks.get_mut(key).unwrap();
            }

            chunk
        };

        if let None = chunk_info.data {
            // TODO: Properly handle network failures.
            println!("Chunk data is empty {}", key);
            return Err(BinsyncError::Unspecified);
        }

        chunk_info.ref_count -= 1;

        if chunk_info.ref_count == 0 {
            self.total_data -= chunk_info.length;

            // TODO: Eject the chunk from memory.
        }

        let data = chunk_info.data.take().unwrap();
        Ok(Rc::new(data))
    }
}

impl Drop for RemoteChunkProvider {
    fn drop(&mut self) {
        self.sender.send(ChunkMessage::Terminate).unwrap();

        if let Some(thread) = self.thread.take() {
            thread.join().unwrap();
        }
    }
}
