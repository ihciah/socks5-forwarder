/// Shared value between kernel and user space.
use std::os::unix::prelude::RawFd;

use redbpf::{Error, HashMap, SockMap};

pub(crate) trait BPFOperator {
    type K;

    fn add(&mut self, fd: RawFd, key: Self::K) -> Result<(), Error>;
    fn delete(&mut self, key: Self::K) -> Result<(), Error>;
}

pub struct Shared<'a, K>
where
    K: Clone,
{
    sockmap: SockMap<'a>,
    idx_map: HashMap<'a, K, u32>,

    idx_slab: slab::Slab<()>,
}

impl<'a, K> Shared<'a, K>
where
    K: Clone,
{
    pub fn new(sockmap: SockMap<'a>, idx_map: HashMap<'a, K, u32>, capacity: usize) -> Self {
        Self {
            sockmap,
            idx_map,
            idx_slab: slab::Slab::with_capacity(capacity),
        }
    }
}

impl<'a, KS> BPFOperator for Shared<'a, KS>
where
    KS: Clone,
{
    type K = KS;

    fn add(&mut self, fd: RawFd, key: Self::K) -> Result<(), Error> {
        let idx = self.idx_slab.insert(()) as u32;
        self.idx_map.set(key, idx);
        self.sockmap.set(idx, fd)
    }

    fn delete(&mut self, key: Self::K) -> Result<(), Error> {
        if let Some(idx) = self.idx_map.get(key.clone()) {
            self.idx_slab.remove(idx as usize);
            self.idx_map.delete(key);
            self.sockmap.delete(idx)
        } else {
            Ok(())
        }
    }
}
