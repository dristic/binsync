use std::path::{Path, PathBuf};

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

    pub fn initiate(&mut self) {
        let hello = Message::Hello(API_VERSION);

        self.socket.send(&hello);
    }
}