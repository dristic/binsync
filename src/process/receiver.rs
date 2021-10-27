use std::{fs::{OpenOptions}, path::{Path, PathBuf}};

use crate::{error::Error, process::FileChecksums};

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

        let response: Message = self.socket.receive()?;
        self.handle_message(&response)?;

        let response: Message = self.socket.receive()?;
        self.handle_message(&response)?;

        Ok(())
    }

    fn handle_message(&mut self, message: &Message) -> Result<(), Error>
    {
        match message {
            Message::Empty => { Ok(()) },
            Message::Hello(version) =>
            {
                println!("Client Hello: {}", version);
                Ok(())
            },
            Message::FileList(list) =>
            {
                println!("Client FileList: {:?}", list);

                let file_info = &list.files[0];
                let path = self.destination.join(Path::new(&file_info.directory));
                let file = OpenOptions::new()
                    .write(true)
                    .read(true)
                    .create(true)
                    .open(path)
                    .expect("Unable to open file for reading.");

                println!("File length: {}", file.metadata().unwrap().len());

                if file.metadata().unwrap().len() == 0 {
                    self.socket.send(&Message::FileChecksums(
                        FileChecksums {
                            id: 0,
                            checksums: vec![],
                        }
                    ))?;
                }

                Ok(())
            },
            Message::FileChecksums(_) => { Ok(()) },
            Message::Shutdown => { Ok(()) },
        }
    }

    pub fn close(&mut self) -> Result<(), Error> {
        let shutdown = Message::Shutdown;

        self.socket.send(&shutdown)?;

        Ok(())
    }
}