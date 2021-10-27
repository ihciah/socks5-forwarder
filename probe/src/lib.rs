#![no_std]

#[derive(Debug, Clone, Copy)]
#[repr(C)]
pub struct IdxMapKey {
    pub addr: u32,
    pub port: u32,
}

pub const MAPPING_CAPACITY: usize = 10240;
