use std::{
    cmp,
    convert::TryFrom,
    fs::{self, File},
    io::{BufWriter, Write},
    path::Path,
};

use rand::{distributions::Alphanumeric, thread_rng, Rng};
use sha2::{Digest, Sha256};

pub struct TestContext {
    base: String,
    /// Set to true to test assertion failures by keeping the files in the dir.
    pub keep_files: bool,
}

impl TestContext {
    pub fn new() -> TestContext {
        let base = thread_rng()
            .sample_iter(&Alphanumeric)
            .take(10)
            .map(char::from)
            .collect();

        let context = TestContext {
            base,
            keep_files: false,
        };

        fs::create_dir_all(context.path("in")).unwrap();
        fs::create_dir_all(context.path("out")).unwrap();

        context
    }

    pub fn path(&self, path: &str) -> String {
        format!("test/{}/{}", self.base, path)
    }

    pub fn write_file(&self, path: &str, size: u64) {
        let path_str = self.path(&path);
        let path = Path::new(&path_str);

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).unwrap();
        }

        let test_file = File::create(path).unwrap();

        let mut writer = BufWriter::new(test_file);

        let mut rng = rand::thread_rng();
        let mut buffer = [0; 1024];
        let mut remaining_size = usize::try_from(size).unwrap();

        while remaining_size > 0 {
            let to_write = cmp::min(remaining_size, buffer.len());
            let buffer = &mut buffer[..to_write];
            rng.fill(buffer);
            writer.write_all(buffer).unwrap();

            remaining_size -= to_write;
        }
    }

    pub fn compare_hashes(&self, a: &str, b: &str) -> bool {
        let source = fs::read(self.path(&a)).unwrap();
        let dest = fs::read(self.path(&b)).unwrap();

        let mut source_hasher = Sha256::new();
        source_hasher.update(source);

        let mut dest_hasher = Sha256::new();
        dest_hasher.update(dest);

        source_hasher.finalize() == dest_hasher.finalize()
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        if !self.keep_files {
            fs::remove_dir_all(self.path("")).unwrap();
        }

        // If we are the last test to finish, cleanup.
        let is_empty = Path::new("test").read_dir().unwrap().next().is_none();
        if is_empty {
            fs::remove_dir("test").unwrap();
        }
    }
}
