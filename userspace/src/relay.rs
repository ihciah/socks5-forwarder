use std::fmt::Display;
use std::os::unix::io::AsRawFd;
use std::os::unix::prelude::RawFd;
use std::sync::{Arc, Mutex};

use futures::{future::BoxFuture, Future};
use probe::IdxMapKey;
use tokio_socks::tcp::Socks5Stream;
use std::net::SocketAddr::{self, V4};
use tokio::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpStream, ToSocketAddrs};
use tokio_socks::IntoTargetAddr;

use crate::shared::BPFOperator;
use crate::{
    shared::Shared,
    utils::{load_bpf, ProxyConfig},
};

pub(crate) struct DirectRelay<T> {
    target_addr: T,
    bpf_shared: Arc<Mutex<Shared<'static, IdxMapKey>>>,
}

pub(crate) struct ProxiedRelay<T> {
    target_addr: T,
    proxy_config: Arc<ProxyConfig>,
    bpf_shared: Arc<Mutex<Shared<'static, IdxMapKey>>>,
}

pub(crate) trait Relay {
    type Fut: Future<Output = anyhow::Result<()>>;

    fn relay(&self, conn: TcpStream) -> Self::Fut;

    fn init_check(&self) {
        if unsafe { libc::geteuid() != 0 } {
            panic!("You must be root to use eBPF!");
        }
    }
}

impl<T> DirectRelay<T>
where
    T: IntoTargetAddr<'static> + Clone + Send + 'static,
{
    pub fn new(target_addr: T) -> Self {
        let bpf_shared = Arc::new(Mutex::new(load_bpf()));
        Self {
            target_addr,
            bpf_shared,
        }
    }
}

impl<T> Relay for DirectRelay<T>
where
    T: ToSocketAddrs + Clone + Send + Display + 'static,
{
    type Fut = BoxFuture<'static, anyhow::Result<()>>;

    fn relay(&self, mut inbound: TcpStream) -> Self::Fut {
        let target = self.target_addr.clone();
        let bpf = self.bpf_shared.clone();

        Box::pin(async move {
            // connect to target
            tracing::info!("Connect target {}", target);
            let mut outbound = TcpStream::connect(target).await?;

            // get inbound and outbound address and fd
            let (inbound_fd, inbound_addr) = (inbound.as_raw_fd(), inbound.peer_addr()?);
            let (outbound_fd, outbound_addr) = (outbound.as_raw_fd(), outbound.local_addr()?);

            let (read_half, write_half) = inbound.split();
            let in_info = ConnInfo {
                fd: inbound_fd,
                addr: inbound_addr,
                read_half,
                write_half,
            };

            let (read_half, write_half) = outbound.split();
            let out_info = ConnInfo {
                fd: outbound_fd,
                addr: outbound_addr,
                read_half,
                write_half,
            };

            // relay
            bpf_relay(bpf, in_info, out_info).await
        })
    }
}

impl<T> ProxiedRelay<T>
where
    T: IntoTargetAddr<'static> + Clone + Send + 'static,
{
    pub fn new(target_addr: T, proxy_config: ProxyConfig) -> Self {
        let proxy_config = Arc::new(proxy_config);
        let bpf_shared = Arc::new(Mutex::new(load_bpf()));
        Self {
            target_addr,
            proxy_config,
            bpf_shared,
        }
    }
}

impl<T> Relay for ProxiedRelay<T>
where
    T: IntoTargetAddr<'static> + Clone + Send + Display + 'static,
{
    type Fut = BoxFuture<'static, anyhow::Result<()>>;

    fn relay(&self, mut inbound: TcpStream) -> Self::Fut {
        let target = self.target_addr.clone();
        let proxy = self.proxy_config.clone();
        let bpf = self.bpf_shared.clone();

        Box::pin(async move {
            // connect to target
            tracing::info!("Connect proxy {}", proxy.address);
            let outbound = TcpStream::connect(&proxy.address).await?;

            // get inbound and outbound address and fd
            let (inbound_fd, inbound_addr) = (inbound.as_raw_fd(), inbound.peer_addr()?);
            let (outbound_fd, outbound_addr) = (outbound.as_raw_fd(), outbound.local_addr()?);

            // wrap outbound to socks5 stream
            tracing::info!("Handshark for target {}", target);
            let mut outbound = match proxy.credential.as_ref() {
                None => Socks5Stream::connect_with_socket(outbound, target).await?,
                Some((username, password)) => {
                    Socks5Stream::connect_with_password_and_socket(
                        outbound,
                        target,
                        username,
                        password,
                    )
                    .await?
                }
            };

            let (read_half, write_half) = inbound.split();
            let in_info = ConnInfo {
                fd: inbound_fd,
                addr: inbound_addr,
                read_half,
                write_half,
            };

            let (read_half, write_half) = outbound.split();
            let out_info = ConnInfo {
                fd: outbound_fd,
                addr: outbound_addr,
                read_half,
                write_half,
            };

            // relay
            bpf_relay(bpf, in_info, out_info).await
        })
    }
}

struct ConnInfo<R, W> {
    fd: RawFd,
    addr: SocketAddr,
    read_half: R,
    write_half: W,
}

async fn bpf_relay<O, IR, IW, OR, OW>(
    bpf: Arc<Mutex<O>>,
    in_conn_info: ConnInfo<IR, IW>,
    out_conn_info: ConnInfo<OR, OW>,
) -> anyhow::Result<()>
where
    O: BPFOperator<K = IdxMapKey>,
    IR: AsyncRead + Unpin,
    IW: AsyncWrite + Unpin,
    OR: AsyncRead + Unpin,
    OW: AsyncWrite + Unpin,
{
    // used for delete from idx_map and sockmap
    let mut inbound_addr_opt = None;
    let mut outbound_addr_opt = None;

    // add socket and key to idx_map and sockmap for ipv4
    // Note: Local port is stored in host byte order while remote port is in network byte order.
    // https://github.com/torvalds/linux/blob/v5.10/include/uapi/linux/bpf.h#L4110
    if let (V4(in_addr), V4(out_addr)) = (in_conn_info.addr, out_conn_info.addr) {
        let inbound_addr = IdxMapKey {
            addr: u32::to_be(u32::from(in_addr.ip().to_owned())),
            port: u32::to_be(in_addr.port().into()),
        };
        let outbound_addr = IdxMapKey {
            addr: u32::to_be(u32::from(out_addr.ip().to_owned())),
            port: out_addr.port().into(),
        };
        inbound_addr_opt = Some(inbound_addr);
        outbound_addr_opt = Some(outbound_addr);
        let mut guard = bpf.lock().unwrap();
        let _ = guard.add(out_conn_info.fd, inbound_addr);
        let _ = guard.add(in_conn_info.fd, outbound_addr);
    }

    // block on copy data
    // Note: Here we copy bidirectional manually, remove from map ASAP to
    // avoid outbound port reuse and packet mis-redirected.
    tracing::info!("Relay started");

    let (mut ri, mut wi) = (in_conn_info.read_half, in_conn_info.write_half);
    let (mut ro, mut wo) = (out_conn_info.read_half, out_conn_info.write_half);
    let client_to_server = async {
        let _ = tokio::io::copy(&mut ri, &mut wo).await;
        tracing::info!("Relay inbound -> outbound finished");
        let _ = wo.shutdown().await;
        if let Some(addr) = inbound_addr_opt {
            let _ = bpf.lock().unwrap().delete(addr);
        }
    };

    let server_to_client = async {
        let _ = tokio::io::copy(&mut ro, &mut wi).await;
        tracing::info!("Relay outbound -> inbound finished");
        let _ = wi.shutdown().await;
        if let Some(addr) = outbound_addr_opt {
            let _ = bpf.lock().unwrap().delete(addr);
        }
    };

    tokio::join!(client_to_server, server_to_client);
    tracing::info!("Relay finished");

    Ok::<(), anyhow::Error>(())
}
