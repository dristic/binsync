use std::{
    io::{Read, Write},
    net::TcpStream,
    str::from_utf8,
};

use crate::{error::Error as BinsynError, process::Socket};
use interprocess::local_socket::LocalSocketStream;

type IoResult<T> = std::io::Result<T>;
pub struct LocalSocketClient {
    pub stream: LocalSocketStream,
}

impl LocalSocketClient {
    pub fn connect() -> IoResult<LocalSocketClient> {
        let stream = LocalSocketStream::connect("/tmp/binsync.sock")?;

        Ok(LocalSocketClient { stream })
    }
}

impl Socket for LocalSocketClient {
    fn send<T: ?Sized>(&mut self, value: &T) -> Result<(), BinsynError>
    where
        T: serde::Serialize,
    {
        let encoded: Vec<u8> =
            bincode::serialize(&value).map_err(|_| BinsynError::new("Could not serialize."))?;

        // Write our length-prefix encoded value.
        let len = (encoded.len() as i32).to_be_bytes();
        self.stream
            .write(&len)
            .map_err(|_| BinsynError::new("Failed to write length."))?;
        self.stream
            .write(&encoded)
            .map_err(|_| BinsynError::new("Failed to write encoded value."))?;

        Ok(())
    }

    fn receive<'a, T>(&mut self) -> Result<T, BinsynError>
    where
        T: serde::de::DeserializeOwned,
    {
        // Read our length-prefix.
        let mut len_buf = [0 as u8; 4];
        self.stream
            .read_exact(&mut len_buf)
            .map_err(|_| BinsynError::new("Failed to read."))?;

        let len = i32::from_be_bytes(len_buf);
        if len > 1000000 {
            panic!("Prefix length too long {:?}", len);
        }

        // Read and decode our value.
        let mut block_buf = vec![0 as u8; len as usize];
        self.stream
            .read_exact(&mut block_buf)
            .map_err(|_| BinsynError::new("Failed to read."))?;

        bincode::deserialize(&block_buf[..]).map_err(|_| BinsynError::new("Failed to deserialize."))
    }
}

pub fn _client_request() {
    match TcpStream::connect("localhost:3333") {
        Ok(mut stream) => {
            println!("Successfully connected to server");

            let msg = b"Hello!";

            stream.write(msg).unwrap();
            println!("Sent Hello, awaiting reply...");

            let mut data = [0 as u8; 6];
            match stream.read_exact(&mut data) {
                Ok(_) => {
                    if &data == msg {
                        println!("Reply is ok!");
                    } else {
                        let text = from_utf8(&data).unwrap();
                        println!("Unexpected reply: {}", text);
                    }
                }
                Err(e) => {
                    println!("Failed to receive data: {}", e);
                }
            }
        }
        Err(e) => {
            println!("Failed to connect: {}", e);
        }
    }
}
