use std::{
    cmp,
    fs::{self, File},
    io::{BufWriter, Write},
};

use rand::Rng;

pub struct TestContext;

impl TestContext {
    pub fn new() -> TestContext {
        fs::create_dir_all("test/in").unwrap();
        fs::create_dir_all("test/out").unwrap();

        let test_file = File::create("test/in/test.bin").unwrap();
        let mut writer = BufWriter::new(test_file);

        let mut rng = rand::thread_rng();
        let mut buffer = [0; 1024];
        let mut remaining_size = 1048576; // 1MB

        while remaining_size > 0 {
            let to_write = cmp::min(remaining_size, buffer.len());
            let buffer = &mut buffer[..to_write];
            rng.fill(buffer);
            writer.write(buffer).unwrap();

            remaining_size -= to_write;
        }

        TestContext
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        fs::remove_dir_all("test").unwrap();
    }
}
