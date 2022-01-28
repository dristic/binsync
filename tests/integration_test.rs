extern crate binsync;

mod common;

#[test]
fn test_empty_destination() {
    let context = common::TestContext::new();

    context.write_file("in/test.bin", 1048576); // 1MB

    binsync::sync(&context.path("in"), &context.path("out")).unwrap();

    assert!(context.compare_hashes("in/test.bin", "out/test.bin"));
}

#[test]
fn test_rand_destination() {
    let context = common::TestContext::new();

    context.write_file("in/test.bin", 1048576); // 1MB
    context.write_file("out/test.bin", 1048577); // 1MB

    binsync::sync(&context.path("in"), &context.path("out")).unwrap();

    assert!(context.compare_hashes("in/test.bin", "out/test.bin"));
}

#[test]
fn test_copy_destination() {
    let context = common::TestContext::new();

    context.write_file("in/test.bin", 1048576); // 1MB
    std::fs::copy(context.path("in/test.bin"), context.path("out/test.bin")).unwrap();

    binsync::sync(&context.path("in"), &context.path("out")).unwrap();

    assert!(context.compare_hashes("in/test.bin", "out/test.bin"));
}
