use std::path::{Path, PathBuf};

use crate::{error::Error};

use super::{API_VERSION, Message, Socket};
pub struct Receiver<T: Socket> {
    destination: PathBuf,
    socket: T
}

impl<T: Socket> Receiver<T> {
    pub fn new<P: AsRef<Path>>(destination: P, socket: T) -> Receiver<T> {
        Receiver {
            destination: destination.as_ref().to_path_buf(),
            socket
        }
    }

    pub fn initiate(&mut self) -> Result<(), Error> {
        let hello = Message::Hello(API_VERSION);

        self.socket.send(&hello)?;

        Ok(())
    }

    pub fn close(&mut self) -> Result<(), Error> {
        let response: Message = self.socket.receive()?;

        match response {
            Message::Empty => {},
            Message::Hello(version) =>
            {
                println!("Client got hello {}", version)
            }
        }

        Ok(())
    }
}