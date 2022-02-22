#[cfg(feature = "network")]
fn main() {
    use std::{
        collections::HashMap,
        env,
        fs::{self, File, OpenOptions},
        io::{Read, Seek, SeekFrom, Write},
        path::Path,
        process,
    };

    use binsync::Manifest;

    use binsync::RemoteManifest;

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
    let manifest = RemoteManifest::from_manifest(manifest);
    let manifest_data = bincode::serialize(&manifest).unwrap();

    if let Err(_) = fs::create_dir("out") {
        println!("Could not create ./out does it already exist?");
        process::exit(1);
    }

    if let Err(_) = fs::write("out/manifest.binsync", manifest_data) {
        println!("Could not write manifest file.");
        process::exit(1);
    }

    let mut chunks = HashMap::new();

    for file_chunk_info in &manifest.source.files {
        let path = from.join(&file_chunk_info.path);

        let mut file = File::open(path).unwrap();

        for chunk in &file_chunk_info.chunks {
            let mut buffer = vec![0; chunk.length as usize];

            file.seek(SeekFrom::Start(chunk.offset)).unwrap();
            file.read_exact(&mut buffer).unwrap();

            chunks.insert(chunk.hash, buffer);
        }
    }

    for pack in manifest.packs {
        let file_name = format!("out/{}.binpack", pack.hash);

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .open(file_name)
            .unwrap();

        for chunk_id in pack.chunks {
            file.write_all(chunks.get_mut(&chunk_id).unwrap()).unwrap();
        }
    }

    println!("Output written to ./out");
}

#[cfg(not(feature = "network"))]
fn main() {
    println!(
        "Network feature is not enabled. Use --features network when running to test this out."
    );
}
