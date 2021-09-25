mod process;
mod net;
mod error;

use adler32::RollingAdler32;
use interprocess::local_socket::{LocalSocketListener, LocalSocketStream};
use md5::Digest;
use process::{Message, Receiver, Sender, Socket};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, convert::TryInto, error::Error, fs::{self, OpenOptions}, io::{BufReader, Read, Write}, net::{Shutdown, TcpListener, TcpStream}, path::Path, sync::mpsc::{self}, thread::{self, Thread}, time::Instant};

use crate::net::client::client_request;
pub struct Opts {
    pub from: String,
    pub to: String,
}

const CHUNK_SIZE: usize = 1100;

fn handle_client(mut stream: TcpStream) {
    let mut data = [0 as u8; 50];
    while match stream.read(&mut data) {
        Ok(size) => {
            stream.write(&data[0..size]).unwrap();
            true
        }
        Err(_) => {
            eprintln!("An error occurred with peer.");
            stream.shutdown(Shutdown::Both).unwrap();
            false
        }
    } {}
}

pub fn server() -> Result<(), Box<dyn Error>> {
    let port = 3333;
    let listener = TcpListener::bind(format!("0.0.0.0:{}", port))?;

    println!("Server listening on port {}", port);

    for stream in listener.incoming() {
        match stream {
            Ok(stream) => {
                println!("New connection: {}", stream.peer_addr().unwrap());
                thread::spawn(move || {
                    handle_client(stream);
                });
            }
            Err(e) => {
                eprintln!("Error handling stream: {}", e);
            }
        }
    }

    drop(listener);

    Ok(())
}

pub fn client(_opts: Opts) -> Result<(), Box<dyn Error>> {
    client_request();

    Ok(())
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct BlockChecksum {
    sum1: u32,
    sum2: [u8; 16],
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct FileInfo {
    name: String,
    directory: String,
}

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct FileList {
    files: Vec<FileInfo>,
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

pub fn copy_file(from: &str, to: &str) -> Result<(), Box<dyn Error>> {
    let from = fs::read(from)?;

    fs::write(to, from)?;

    Ok(())
}

type IoResult<T> = std::io::Result<T>;

struct LocalSocketHost {
    listener: LocalSocketListener
}

impl LocalSocketHost {
    fn bind() -> IoResult<LocalSocketHost> {
        let listener = LocalSocketListener::bind("/tmp/binsync.sock")?;

        Ok(LocalSocketHost { listener })
    }
}

impl Socket<Message> for LocalSocketHost {
    fn send(&self, message: Message) -> Result<(), error::Error> {
        Ok(())
    }
}

struct LocalSocketClient {
    stream: LocalSocketStream
}

impl LocalSocketClient {
    fn connect() -> IoResult<LocalSocketClient> {
        let stream = LocalSocketStream::connect("/tmp/binsync.sock")?;

        Ok(LocalSocketClient { stream })
    }
}

impl Socket<Message> for LocalSocketClient {
    fn send(&self, message: Message) -> Result<(), error::Error> {
        Ok(())
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
    let local_socket_host = LocalSocketHost::bind()?;
    let sender = Sender::new(&from_path, local_socket_host);

    let local_socket_client = LocalSocketClient::connect()?;
    let receiver = Receiver::new(&to_path, local_socket_client);

    // Generate file list.
    let file_list = sender.get_file_list();

    for file in file_list.files {
        println!("{} {}", file.name, file.directory);
    }

    // Create generator, sender, and receiver in pipeline.

    // if to_path.exists() {
    //     sync_file(&opts.from, &opts.to)
    // } else {
    //     copy_file(&opts.from, &opts.to)
    // }

    Ok(())
}
