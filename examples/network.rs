extern crate binsync;

#[cfg(feature = "network")]
fn main() {
    use std::{env, process, time::Instant};

    let args: Vec<String> = env::args().collect();

    if args.len() < 3 {
        println!("Requires a remote location and destination folder.");
        process::exit(1);
    }

    let now = Instant::now();

    if let Err(e) = sync_network(args) {
        println!("Encountered error syncing {}", e);
        process::exit(1);
    }

    println!("Sync took {}s", now.elapsed().as_secs());
}

#[cfg(feature = "network")]
fn sync_network(args: Vec<String>) -> Result<(), Box<dyn std::error::Error>> {
    use binsync::{RemoteManifest, Syncer};
    use std::path::Path;

    let url = reqwest::Url::parse(&args[1])?;

    let to_path = Path::new(&args[2]);

    let manifest_url = url.join("manifest.binsync")?;

    println!("Fetching manifest from {}", manifest_url);

    let response = reqwest::blocking::get(manifest_url)?;
    let data = response.bytes()?;

    let manifest: RemoteManifest = bincode::deserialize(&data)?;
    let provider = binsync::RemoteChunkProvider::new(url.as_str(), &manifest)?;

    let mut syncer = Syncer::new(to_path, provider, manifest.source);
    syncer.sync()?;

    Ok(())
}

#[cfg(not(feature = "network"))]
fn main() {
    println!(
        "Network feature is not enabled. Use --features network when running to test this out."
    );
}
