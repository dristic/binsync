use std::rc::Rc;

use crate::{BinsyncError, ChunkProvider};

/// A simple remote chunk provider from the given URI. Will make GET network
/// requests against the URI with the chunk hash appended to the end. Will also
/// attempt to cache chunks in LRU with a given memory budget.
pub struct RemoteChunkProvider {}

impl ChunkProvider for RemoteChunkProvider {
    fn set_plan(&mut self, _plan: &super::SyncPlan) {
        todo!()
    }

    fn get_chunk(&mut self, _key: &u64) -> Result<Rc<Vec<u8>>, BinsyncError> {
        todo!()
    }
}
