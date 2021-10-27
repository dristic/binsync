mod process;
mod net;
mod error;

use adler32::RollingAdler32;
use interprocess::local_socket::{LocalSocketListener, LocalSocketStream};
use md5::Digest;
use process::{Receiver, Sender, Socket};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::TryInto, error::Error, fs::{self, OpenOptions}, io::{BufReader, Read, Write}, path::Path, sync::mpsc::{self}, thread::{self}, time::Instant};

use crate::net::client::client_request;
pub struct Opts {
    pub from: String,
    pub to: String,
}

const CHUNK_SIZE: usize = 1100;

pub fn client(_opts: Opts) -> Result<(), Box<dyn Error>> {
    client_request();

    Ok(())
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct BlockChecksum {
    sum1: u32,
    sum2: [u8; 16],
}

struct ShutdownMessage {}

pub fn test_server() -> Result<(), Box<dyn Error>> {
    let dig = md5::compute(b"1234567");

    let checksums = BlockChecksum {
        sum1: 12345,
        sum2: *dig,
    };

    let encoded: Vec<u8> = bincode::serialize(&checksums).unwrap();

    let (sender, receiver) = mpsc::channel::<ShutdownMessage>();

    ctrlc::set_handler(move || {
        println!("Sending shutdown message!");
        sender.send(ShutdownMessage{}).unwrap();
    }).expect("Error setting Ctrl-C handler");

    let listener = LocalSocketListener::bind("/tmp/binsync.sock")?;
    thread::spawn(move || {
        for connection in listener.incoming() {
            let mut conn = match connection {
                Ok(connection) => {
                    connection
                },
                Err(e) => {
                    eprintln!("Incoming connection failed: {}", e);
                    break;
                }
            };
    
            println!("Incoming connection!");
    
            // Write our length-prefix encoded value.
            let len = (encoded.len() as i32).to_be_bytes();
            conn.write(&len).unwrap();
            conn.write(&encoded).unwrap();
    
            // Read our length-prefix.
            let mut conn = BufReader::new(conn);
            let mut len_buf = [0 as u8; 4];
            conn.read_exact(&mut len_buf).unwrap();
    
            let len = i32::from_be_bytes(len_buf);
            println!("Got length {} {} {}", len, len_buf.len(), encoded.len());
            if len > 100000 {
                panic!("Prefix length too long {:?}", len);
            }
    
            // Read and decode our value.
            let mut block_buf = vec![0 as u8; len as usize];
            conn.read_exact(&mut block_buf).unwrap();
            let decoded: BlockChecksum = bincode::deserialize(&block_buf[..]).unwrap();
    
            println!("Client anwered: {:?}", decoded);
        }
    });

    receiver.recv()?;

    Ok(())
}

pub fn test_client() -> Result<(), Box<dyn Error>> {
    let dig = md5::compute(b"1234567");

    let checksums = BlockChecksum {
        sum1: 12345,
        sum2: *dig,
    };

    let encoded: Vec<u8> = bincode::serialize(&checksums).unwrap();

    let mut conn = LocalSocketStream::connect("/tmp/binsync.sock")?;

    // Write our length-prefix encoded value.
    let len = (encoded.len() as i32).to_be_bytes();
    conn.write(&len)?;
    conn.write(&encoded)?;

    // Read our length-prefix.
    let mut conn = BufReader::new(conn);
    let mut len_buf = [0 as u8; 4];
    conn.read_exact(&mut len_buf)?;

    let len = i32::from_be_bytes(len_buf);
    println!("Got length {} {}", len, encoded.len());
    if len > 100000 {
        panic!("Prefix length too long {:?}", len);
    }

    // Read and decode our value.
    let mut block_buf = vec![0 as u8; len as usize];
    conn.read_exact(&mut block_buf)?;
    let decoded: BlockChecksum = bincode::deserialize(&block_buf[..]).unwrap();

    println!("Server anwered: {:?}", decoded);

    Ok(())
}

fn sync_file(from: &str, to: &str) -> Result<(), Box<dyn Error>> {
    let start = Instant::now();
    let to_bytes = fs::read(to)?;
    println!("Read took {:?}", start.elapsed());

    // Generate checksums for what the receiver has.
    let start = Instant::now();
    let mut checksums: HashMap<u32, (Digest, &[u8])> = HashMap::new();
    for (_, chunk) in to_bytes.chunks(CHUNK_SIZE).enumerate() {
        let mut adler = simd_adler32::Adler32::new();
        adler.write(chunk);

        let digest = md5::compute(chunk);

        checksums.insert(adler.finish(), (digest, &chunk));
    }
    println!("Checksum generation took {:?}", start.elapsed());

    // Read the sending file for comparison.
    let start = Instant::now();
    let from = fs::read(from)?;
    println!("Read took {:?}", start.elapsed());

    // Loop through the sending file, figuring out how to reconstruct on the receiving end.
    let start = Instant::now();
    let mut adler = RollingAdler32::new();
    let mut s = 0;
    let mut sent_bytes = 0;
    let mut reconstructed = Vec::new();
    for (i, b) in from.iter().enumerate() {
        adler.update(*b);

        let size = i - s + 1;
        if size > CHUNK_SIZE {
            // The oldest byte needs to be sent.
            let removed_byte = from[s];
            adler.remove(CHUNK_SIZE + 1, removed_byte);

            reconstructed.push(from[s]);

            s = s + 1;
            sent_bytes = sent_bytes + 1;
        }

        let size = i - s + 1;
        if size == CHUNK_SIZE {
            let hash = adler.hash();

            if checksums.contains_key(&hash) {
                let (to_digest, chunk) = checksums[&hash];
                let from_chunk = &from[s..i + 1];
                let from_digest = md5::compute(from_chunk);

                if from_digest == to_digest {
                    reconstructed.extend(chunk.iter());

                    s = i + 1;
                    adler = RollingAdler32::new();
                    sent_bytes = sent_bytes + 4;
                }
            }
        }
    }
    println!("Checksum enumeration took {:?}", start.elapsed());

    reconstructed.extend_from_slice(&from[s..]);

    // Write the new bytes to the file and truncate the rest.
    let mut to = OpenOptions::new().write(true).open(to)?;
    to.write(&reconstructed)?;
    to.set_len(from.len().try_into().unwrap())?;

    println!("Sent a total of {} bytes", sent_bytes);
    println!("Original is a total of {} bytes", from.len());
    println!("Reconstructed is a total of {} bytes", reconstructed.len());

    let hash1 = md5::compute(from);
    let hash2 = md5::compute(reconstructed);
    println!("Are files similar? {}", hash1 == hash2);

    Ok(())
}

type IoResult<T> = std::io::Result<T>;

struct LocalSocketClient {
    stream: LocalSocketStream
}

impl LocalSocketClient {
    fn connect() -> IoResult<LocalSocketClient> {
        let stream = LocalSocketStream::connect("/tmp/binsync.sock")?;

        Ok(LocalSocketClient { stream })
    }
}

impl Socket for LocalSocketClient {
    fn send<T: ?Sized>(&mut self, value: &T) -> Result<(), error::Error>
    where
        T: serde::Serialize,
    {
        let encoded: Vec<u8> = bincode::serialize(&value)
            .map_err(|_| { error::Error::new("Could not serialize.") })?;

        // Write our length-prefix encoded value.
        let len = (encoded.len() as i32).to_be_bytes();
        self.stream.write(&len)
            .map_err(|_| error::Error::new("Failed to write length."))?;
        self.stream.write(&encoded)
            .map_err(|_| error::Error::new("Failed to write encoded value."))?;

        Ok(())
    }

    fn receive<'a, T>(&mut self) -> Result<T, error::Error>
    where
        T: serde::de::DeserializeOwned
    {
        // Read our length-prefix.
        let mut len_buf = [0 as u8; 4];
        self.stream.read_exact(&mut len_buf)
            .map_err(|_| error::Error::new("Failed to read."))?;

        let len = i32::from_be_bytes(len_buf);
        if len > 100000 {
            panic!("Prefix length too long {:?}", len);
        }

        // Read and decode our value.
        let mut block_buf = vec![0 as u8; len as usize];
        self.stream.read_exact(&mut block_buf)
            .map_err(|_| error::Error::new("Failed to read."))?;

        bincode::deserialize(&block_buf[..])
            .map_err(|_| error::Error::new("Failed to deserialize."))
    }
}

pub fn sync(opts: Opts) -> Result<(), Box<dyn std::error::Error>> {
    let to_path = Path::new(&opts.to);

    // Validate options.
    let from_path = Path::new(&opts.from);
    if !from_path.exists() {
        return Err(Box::new(error::Error::new("Cannot find from file.")));
    }

    // Negotiate protocol (future)

    // Establish connection
    let listener = LocalSocketListener::bind("/tmp/binsync.sock")?;
    let client = LocalSocketClient::connect()?;
    let host = LocalSocketClient{
        stream: listener.accept()?
    };

    let mut sender = Sender::new(&from_path, host);
    let mut receiver = Receiver::new(&to_path, client);

    // Initiate the transfer.
    let sender_thread = thread::spawn(move || -> Result<(), error::Error> {
        sender.listen()
            .map_err(|_| error::Error::new("Failed to listen."))
    });

    receiver.initiate()?;
    receiver.close()?;

    sender_thread.join()
        .unwrap()
        .map_err(|_| error::Error::new("Thread join failed."))?;

    Ok(())
}
