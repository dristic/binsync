use std::fs;

use sha2::{Digest, Sha256};

extern crate binsync;

mod common;

#[test]
fn test_generate() {
    let _context = common::TestContext::new();

    let opts = binsync::Opts {
        from: String::from("./test/in"),
        to: String::from("./test/out"),
    };

    binsync::generate(opts).unwrap();

    let source = fs::read("./test/in/test.bin").unwrap();
    let dest = fs::read("./test/out/test.bin").unwrap();

    assert_eq!(source.len(), dest.len());

    let mut source_hasher = Sha256::new();
    source_hasher.update(source);

    let mut dest_hasher = Sha256::new();
    dest_hasher.update(dest);

    assert_eq!(source_hasher.finalize(), dest_hasher.finalize());
}
