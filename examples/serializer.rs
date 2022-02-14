use std::{
    env,
    fs::{self, File},
    io::{Read, Seek, SeekFrom},
    path::Path,
    process,
};

use binsync::Manifest;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 2 {
        println!("Requires a source folder argument.");
        process::exit(1);
    }

    let from = Path::new(&args[1]);
    if !from.exists() {
        println!("Could not find source {:?}", from);
        process::exit(1);
    }

    let manifest = Manifest::from_path(from);
    let manifest_data = bincode::serialize(&manifest).unwrap();

    if let Err(_) = fs::create_dir("out") {
        println!("Could not create ./out does it already exist?");
        process::exit(1);
    }

    if let Err(_) = fs::write("out/manifest.binsync", manifest_data) {
        println!("Could not write manifest file.");
        process::exit(1);
    }

    for (file_info, chunks) in &manifest.files {
        let path = from.join(file_info);
        let mut file = File::open(path).unwrap();

        for chunk in chunks {
            let mut buffer = vec![0; chunk.length as usize];

            file.seek(SeekFrom::Start(chunk.offset)).unwrap();
            file.read_exact(&mut buffer).unwrap();

            let file_name = format!("out/{}.chunk", chunk.hash);
            fs::write(file_name, buffer).unwrap();
        }
    }

    println!("Output written to ./out");
}
