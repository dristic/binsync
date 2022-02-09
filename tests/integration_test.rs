use std::fs;

extern crate binsync;

mod common;

#[test]
/// Tests a basic copy and paste. No file exists at the destination.
fn test_empty_destination() {
    let context = common::TestContext::new();

    context.write_file("in/test.bin", 1048576); // 1MB

    binsync::sync(&context.path("in"), &context.path("out")).unwrap();

    assert!(context.compare_hashes("in/test.bin", "out/test.bin"));
}

#[test]
/// Write into a file deep into a folder hierarchy. Catches issues when we
/// assume certain folders exist at the destination.
fn test_empty_destination_complex() {
    let context = common::TestContext::new();

    context.write_file("in/foo/bar/baz/test.bin", 1048576); // 1MB

    binsync::sync(&context.path("in"), &context.path("out")).unwrap();

    assert!(context.compare_hashes("in/foo/bar/baz/test.bin", "out/foo/bar/baz/test.bin"));
}

#[test]
/// Randomize the destination file, along with making it longer. Ensures
/// proper truncation and modification of the destination file.
fn test_rand_destination() {
    let context = common::TestContext::new();

    context.write_file("in/test.bin", 1048576); // 1MB
    context.write_file("out/test.bin", 1048677); // 1MB

    binsync::sync(&context.path("in"), &context.path("out")).unwrap();

    assert!(context.compare_hashes("in/test.bin", "out/test.bin"));
}

#[test]
/// Two exact same files that should incur no copies.
fn test_copy_destination() {
    let context = common::TestContext::new();

    context.write_file("in/test.bin", 1048576); // 1MB
    fs::copy(context.path("in/test.bin"), context.path("out/test.bin")).unwrap();

    binsync::sync(&context.path("in"), &context.path("out")).unwrap();

    assert!(context.compare_hashes("in/test.bin", "out/test.bin"));
}

#[test]
/// Forces us to copy chunks from the existing destination file and reuse them.
/// This rarely happens in the other tests as chunks do not move throughout the
/// file we are copying from/to.
fn test_copy_destination_padded() {
    let context = common::TestContext::new();

    context.write_file("in/test.bin", 1048576); // 1MB
    fs::copy(context.path("in/test.bin"), context.path("out/test.bin")).unwrap();

    // Change the in file to contain its data embedded inside itself. This is
    // not done optimally but conveniently.
    let mut data = fs::read(context.path("in/test.bin")).unwrap();
    let mut copy = data.clone();
    let mut copy2 = copy.split_off(copy.len() / 2);

    copy2.append(&mut data);
    copy2.append(&mut copy);
    fs::write(context.path("in/test.bin"), copy2).unwrap();

    binsync::sync(&context.path("in"), &context.path("out")).unwrap();

    assert!(context.compare_hashes("in/test.bin", "out/test.bin"));
}
