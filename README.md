# Binsync

![CI](https://github.com/dristic/binsync/actions/workflows/rust.yml/badge.svg)
![Crates.io](https://img.shields.io/crates/v/binsync)

A library for syncing large binary data between two locations. Utilizes content defined chunking to break files down into chunks and transfer only the deltas between two locations. The library uses a static manifest format as the source meaning you can generate the manifest and chunks ahead of time and serve them to a large number of destinations.

## Building and Testing
```
$ cargo build
$ cargo test --test integration_test
```

## Getting Started

The library has three major components: the manifest, the chunk provider, and the syncer.

### Manifest

The manifest describes the files and chunks from a given source directory. It has a list of all files and which chunks appear in which order in each file. This is meant to be generated ahead of time and the same manifest can be used for a large number of destinations. Once a manifest is generated for a given folder it can be serialized into any number of formats as long as your destination, remote or local, understands how to parse the manifest for use in syncing.

### Chunk Provider

The chunk provider fetches chunk contents for the syncer. This is a trait that can be implemented to suit your needs. There are a few implementations provided:
- `CachingChunkProvider` is optimal for local syncing on the same machine. It attempts to read ahead and cache chunks as quickly as possible to maximize memory and disk I/O usage.
- `RemoteChunkProvider` is useful when the source and destination are on different machines. It works similar to the caching provider but fetches chunks from a remote base URL.

Your application may have different needs i.e. fetching from S3, making authenticated requests, fetching from multiple sources, etc.

### Syncer

The syncer takes a manifest and chunk provider and runs the syncing logic to transform the target destination into an exact binary replica of the source. It reuses chunks that already exist in the destination folder to reduce the amount of data needed to transfer over the network.

### Example

```rust
use binsync::{Manifest, CachingChunkProvider, Syncer};

let manifest = Manifest::from_path("foo/source");

let basic_provider = CachingChunkProvider::new("foo/source");

let mut syncer = Syncer::new("foo/destination", basic_provider, manifest);
syncer.sync().unwrap();
```

The integration tests are a great way to understand how things work. They generate a test folder with in/out folders and sync between the two. Inside `tests/common.rs` you can comment out the `Drop` logic to prevent the tests from cleaning up the folders after running.

## Contributing

Pull requests and issues are welcome. Currently the response time is best effort.
