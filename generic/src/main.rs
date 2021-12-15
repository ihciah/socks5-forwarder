use std::fmt::Debug;
use std::sync::Arc;

use std::time::Duration;
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use tokio_socks::tcp::Socks5Stream;
use tokio_socks::IntoTargetAddr;
use tokio_stream::wrappers::TcpListenerStream;
use tokio_stream::StreamExt;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

use clap::Parser;

const DEFAULT_KEEPALIVE_TIMEOUT: Duration = Duration::from_secs(15);

#[derive(Parser)]
#[clap(version, author, about)]
struct Opts {
    #[clap(short, long, default_value = "127.0.0.1:8000", help = "listen address")]
    listen: String,
    #[clap(short, long, help = "target address, like 1.1.1.1:443")]
    target: String,
    #[clap(
        long,
        help = "socks5 proxy address, like 10.0.0.1:8080(leave blank for direct proxy)"
    )]
    proxy_addr: Option<String>,
    #[clap(long, help = "socks5 proxy username")]
    proxy_user: Option<String>,
    #[clap(long, help = "socks5 proxy password")]
    proxy_pass: Option<String>,
}

#[tokio::main]
async fn main() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();

    let opt = Opts::parse();
    if let Some(address) = opt.proxy_addr {
        let proxy_config = ProxyConfig {
            address,
            credential: match (opt.proxy_user, opt.proxy_pass) {
                (Some(u), Some(p)) => Some((u, p)),
                (Some(u), None) => Some((u, String::default())),
                _ => None,
            },
        };
        tracing::info!("Will use socks proxy {}", proxy_config.address);
        serve_with_proxy(opt.listen, opt.target, proxy_config)
            .await
            .expect("unexpected error");
    } else {
        serve(opt.listen, opt.target)
            .await
            .expect("unexpected error");
    }
}

#[derive(Debug, Clone)]
struct ProxyConfig {
    address: String,
    credential: Option<(String, String)>,
}

async fn serve_with_proxy<L, T>(
    listen_addr: L,
    target_addr: T,
    proxy: ProxyConfig,
) -> anyhow::Result<()>
where
    L: ToSocketAddrs + Debug + 'static,
    T: IntoTargetAddr<'static> + Clone + Send + 'static,
{
    tracing::info!("Listening at {:?}", listen_addr);
    let mut listener_stream = TcpListenerStream::new(TcpListener::bind(listen_addr).await?);
    let proxy = Arc::new(proxy);

    loop {
        match listener_stream.try_next().await {
            Ok(Some(conn)) => {
                tracing::info!("Receive new incoming connection");
                #[cfg(unix)]
                set_tcp_keepalive(&conn, Some(DEFAULT_KEEPALIVE_TIMEOUT))?;
                let target_addr = target_addr.clone();
                let proxy = proxy.clone();
                tokio::spawn(async move { relay_with_proxy(conn, target_addr, proxy).await });
            }
            Ok(None) => {
                tracing::info!("Listener closed");
                return Ok(());
            }
            Err(e) => {
                tracing::error!("Receiving incoming connection in failure: {}", e);
            }
        }
    }
}

async fn serve<L, T>(listen_addr: L, target_addr: T) -> anyhow::Result<()>
where
    L: ToSocketAddrs + Debug + 'static,
    T: ToSocketAddrs + Clone + Send + 'static,
{
    tracing::info!("Listening at {:?}", listen_addr);
    let mut listener_stream = TcpListenerStream::new(TcpListener::bind(listen_addr).await?);

    loop {
        match listener_stream.try_next().await {
            Ok(Some(conn)) => {
                #[cfg(unix)]
                set_tcp_keepalive(&conn, Some(DEFAULT_KEEPALIVE_TIMEOUT))?;
                tracing::info!("Receive new incoming connection");
                let target_addr = target_addr.clone();
                tokio::spawn(async move { relay(conn, target_addr).await });
            }
            Ok(None) => {
                tracing::info!("Listener closed");
                return Ok(());
            }
            Err(e) => {
                tracing::error!("Receiving incoming connection in failure: {}", e);
            }
        }
    }
}

async fn relay_with_proxy<'a, T>(
    mut inbound: TcpStream,
    target_addr: T,
    proxy: Arc<ProxyConfig>,
) -> anyhow::Result<()>
where
    T: IntoTargetAddr<'a> + Clone,
{
    let proxy_stream = TcpStream::connect(&proxy.address).await?;
    #[cfg(unix)]
    set_tcp_keepalive(&proxy_stream, Some(DEFAULT_KEEPALIVE_TIMEOUT))?;
    let mut outbound = match proxy.credential.as_ref() {
        None => Socks5Stream::connect_with_socket(proxy_stream, target_addr).await?,
        Some((username, password)) => {
            Socks5Stream::connect_with_password_and_socket(
                proxy_stream,
                target_addr,
                username,
                password,
            )
            .await?
        }
    };

    tracing::info!("Start relay");
    tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await?;

    tracing::info!("Relay finished");
    Ok(())
}

async fn relay<'a, T>(mut inbound: TcpStream, target_addr: T) -> anyhow::Result<()>
where
    T: ToSocketAddrs + Clone,
{
    let mut outbound = TcpStream::connect(target_addr).await?;
    #[cfg(unix)]
    set_tcp_keepalive(&outbound, Some(DEFAULT_KEEPALIVE_TIMEOUT))?;

    tracing::info!("Start relay");
    tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await?;

    tracing::info!("Relay finished");
    Ok(())
}

#[cfg(unix)]
fn set_tcp_keepalive(
    stream: &tokio::net::TcpStream,
    keepalive_duration: Option<Duration>,
) -> anyhow::Result<()> {
    use std::os::unix::io::{AsRawFd, FromRawFd, IntoRawFd};
    let socket = unsafe { socket2::Socket::from_raw_fd(stream.as_raw_fd()) };
    keepalive_duration
        .map(|duration| socket2::TcpKeepalive::new().with_time(duration))
        .map(|ref keepalive| socket.set_tcp_keepalive(keepalive))
        .transpose()?;
    let _ = socket.into_raw_fd();
    Ok(())
}
