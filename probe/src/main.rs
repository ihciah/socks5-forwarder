#![allow(unused_attributes)]
#![no_std]
#![no_main]

use core::ptr;
use memoffset::offset_of;
use redbpf_probes::sockmap::prelude::*;

use probe::IdxMapKey;

program!(0xFFFFFFFE, "GPL");

#[map(link_section = "maps/sockmap")]
static mut SOCKMAP: SockMap = SockMap::with_max_entries(probe::MAPPING_CAPACITY as u32);

#[map(link_section = "maps/idx_map")]
static mut IDX_MAP: HashMap<IdxMapKey, u32> = HashMap::with_max_entries(probe::MAPPING_CAPACITY as u32);

#[stream_parser]
fn parse_message_boundary(skb: SkBuff) -> StreamParserResult {
    let len: u32 = unsafe {
        let addr = (skb.skb as usize + offset_of!(__sk_buff, len)) as *const u32;
        ptr::read(addr)
    };
    Ok(StreamParserAction::MessageLength(len))
}

#[stream_verdict]
fn verdict(skb: SkBuff) -> SkAction {
    let (ip, port, lip, lport) = unsafe {
        let remote_ip_addr = (skb.skb as usize + offset_of!(__sk_buff, remote_ip4)) as *const u32;
        let remote_port_addr = (skb.skb as usize + offset_of!(__sk_buff, remote_port)) as *const u32;
        let local_ip_addr = (skb.skb as usize + offset_of!(__sk_buff, local_ip4)) as *const u32;
        let local_port_addr = (skb.skb as usize + offset_of!(__sk_buff, local_port)) as *const u32;
        (ptr::read(remote_ip_addr), ptr::read(remote_port_addr), ptr::read(local_ip_addr), ptr::read(local_port_addr))
    };

    let key = IdxMapKey { addr: ip, port };
    if let Some(idx) = unsafe {IDX_MAP.get(&key)} {
        return match unsafe { SOCKMAP.redirect(skb.skb as *mut _, *idx) } {
            Ok(_) => {
                SkAction::Pass
            },
            Err(_) => {
                SkAction::Drop
            },
        };
    }
    let key = IdxMapKey { addr: lip, port: lport };
    if let Some(idx) = unsafe {IDX_MAP.get(&key)} {
        return match unsafe { SOCKMAP.redirect(skb.skb as *mut _, *idx) } {
            Ok(_) => {
                SkAction::Pass
            },
            Err(_) => {
                SkAction::Drop
            },
        };
    }
    SkAction::Pass
}
