use probe::{IdxMapKey, MAPPING_CAPACITY};
use redbpf::{HashMap, SockMap, load::Loader};

use crate::shared::Shared;

#[derive(Debug, Clone)]
pub(crate) struct ProxyConfig {
    pub(crate) address: String,
    pub(crate) credential: Option<(String, String)>,
}

pub(crate) fn load_bpf() -> Shared<'static, IdxMapKey> {
    let loaded = Loader::load(include_bytes!(concat!(
        env!("OUT_DIR"),
        "/target/bpf/programs/probes/probes.elf"
    )))
    .expect("error loading BPF program");
    // let loaded = Loader::load(b"").expect("error loading BPF program");
    let loaded_leak = Box::leak(Box::new(loaded));
    let sockmap = SockMap::new(loaded_leak.map("sockmap").expect("sockmap not found")).unwrap();
    let idx_map =
        HashMap::<IdxMapKey, u32>::new(loaded_leak.map("idx_map").expect("idx map not found"))
            .unwrap();
    loaded_leak
        .stream_parsers()
        .next()
        .unwrap()
        .attach_sockmap(&sockmap)
        .expect("Attaching sockmap to stream parsers failed");
    loaded_leak
        .stream_verdicts()
        .next()
        .unwrap()
        .attach_sockmap(&sockmap)
        .expect("Attaching sockmap to stream verdicts failed");
    Shared::new(sockmap, idx_map, MAPPING_CAPACITY)
}
