use std::path::{Path, PathBuf};

use super::{Message, Socket};
pub struct Receiver<T: Socket<Message>> {
    destination: PathBuf,
    socket: T
}

impl<T: Socket<Message>> Receiver<T> {
    pub fn new<P: AsRef<Path>>(destination: P, socket: T) -> Receiver<T> {
        Receiver {
            destination: destination.as_ref().to_path_buf(),
            socket
        }
    }
}