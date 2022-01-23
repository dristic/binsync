extern crate binsync;

mod common;

#[test]
fn test_empty_destination() {
    let context = common::TestContext::new();

    context.write_file("in/test.bin", 1048576); // 1MB

    let opts = binsync::Opts {
        from: context.path("in"),
        to: context.path("out"),
    };

    binsync::generate(opts).unwrap();

    assert!(context.compare_hashes("in/test.bin", "out/test.bin"));
}

#[test]
fn test_rand_destination() {
    let context = common::TestContext::new();

    context.write_file("in/test.bin", 1048576); // 1MB
    context.write_file("out/test.bin", 1048577); // 1MB

    let opts = binsync::Opts {
        from: context.path("in"),
        to: context.path("out"),
    };

    binsync::generate(opts).unwrap();

    assert!(context.compare_hashes("in/test.bin", "out/test.bin"));
}
