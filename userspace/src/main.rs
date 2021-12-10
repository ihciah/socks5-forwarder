use std::fmt::Debug;

use clap::Parser;
use relay::{DirectRelay, ProxiedRelay, Relay};
use tokio::net::{TcpListener, ToSocketAddrs};
use tracing::Level;
use tracing_subscriber::FmtSubscriber;
use utils::ProxyConfig;

mod relay;
mod shared;
mod utils;

#[derive(Parser)]
#[clap(version, author, about)]
struct Opts {
    #[clap(
        short,
        long,
        default_value = "127.0.0.1:8000",
        help = "listen address"
    )]
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
        let relay = ProxiedRelay::new(opt.target, proxy_config);
        serve(opt.listen, relay).await.expect("unexpected error");
    } else {
        let relay = DirectRelay::new(opt.target);
        serve(opt.listen, relay).await.expect("unexpected error");
    }
}

async fn serve<L, R>(listen_addr: L, relay: R) -> anyhow::Result<()>
where
    L: ToSocketAddrs + Debug + 'static,
    R: Relay,
    <R as Relay>::Fut: Send + 'static,
{
    relay.init_check();

    tracing::info!("Listening at {:?}", listen_addr);
    let listener = TcpListener::bind(listen_addr).await?;
    loop {
        match listener.accept().await {
            Ok((conn, _)) => {
                tracing::info!("Accept new incoming connection");
                tokio::spawn(relay.relay(conn));
            }
            Err(e) => {
                tracing::error!("Accept error: {}", e);
            }
        }
    }
}
