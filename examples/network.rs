extern crate binsync;

use std::{env, error::Error, path::Path, process};

use binsync::{Manifest, RemoteChunkProvider, Syncer};
use reqwest::Url;

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        println!("Requires a remote location and destination folder.");
        process::exit(1);
    }

    if let Err(e) = sync_network(args) {
        println!("Encountered error syncing {}", e);
        process::exit(1);
    }
}

fn sync_network(args: Vec<String>) -> Result<(), Box<dyn Error>> {
    let url = Url::parse(&args[1])?;

    let to_path = Path::new(&args[2]);

    let manifest_url = url.join("manifest.binsync")?;

    println!("Fetching manifest from {}", manifest_url);

    let response = reqwest::blocking::get(manifest_url)?;
    let data = response.bytes()?;

    let manifest: Manifest = bincode::deserialize(&data)?;
    let provider = RemoteChunkProvider::new(url.as_str());

    let mut syncer = Syncer::new(to_path, provider, manifest);
    syncer.sync()?;

    Ok(())
}
