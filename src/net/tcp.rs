use std::net::{Shutdown, TcpListener, TcpStream};

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