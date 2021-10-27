use std::fmt::Debug;
use std::sync::Arc;

use clap::{App, Arg};
use tokio::net::{TcpListener, TcpStream, ToSocketAddrs};
use tokio_socks::tcp::Socks5Stream;
use tokio_socks::IntoTargetAddr;
use tokio_stream::wrappers::TcpListenerStream;
use tokio_stream::StreamExt;
use tracing::Level;
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .finish();
    tracing::subscriber::set_global_default(subscriber).unwrap();

    let matches = App::new("Socks5 Forwarder")
        .version(clap::crate_version!())
        .author(clap::crate_authors!(", "))
        .about("Forward incoming connections to socks5 proxy")
        .arg(
            Arg::with_name("listen")
                .short("l")
                .long("listen")
                .takes_value(true)
                .default_value("127.0.0.1:8000")
                .help("listen address, like 127.0.0.1:8000"),
        )
        .arg(
            Arg::with_name("target")
                .short("t")
                .long("target")
                .takes_value(true)
                .help("target address, like 1.1.1.1:443"),
        )
        .arg(
            Arg::with_name("proxy-addr")
                .long("proxy-addr")
                .takes_value(true)
                .help("socks5 proxy address, like 10.0.0.1:8080"),
        )
        .arg(
            Arg::with_name("proxy-username")
                .long("proxy-user")
                .takes_value(true)
                .help("socks5 proxy username, can be left blank"),
        )
        .arg(
            Arg::with_name("proxy-password")
                .long("proxy-pass")
                .takes_value(true)
                .help("socks5 proxy password, can be left blank"),
        )
        .get_matches();

    let listen_addr = matches.value_of("listen").unwrap().to_string();
    let target_addr = matches.value_of("target").unwrap().to_string();
    let proxy_addr = matches.value_of("proxy-addr").unwrap().to_string();
    let proxy_username = matches.value_of("proxy-username");
    let proxy_password = matches.value_of("proxy-password");

    let proxy_config = ProxyConfig {
        address: proxy_addr,
        credential: match (proxy_username, proxy_password) {
            (Some(u), Some(p)) => Some((u.to_string(), p.to_string())),
            _ => None,
        },
    };
    serve(listen_addr, target_addr, proxy_config)
        .await
        .expect("unexpected error")
}

#[derive(Debug, Clone)]
struct ProxyConfig {
    address: String,
    credential: Option<(String, String)>,
}

async fn serve<L, T>(listen_addr: L, target_addr: T, proxy: ProxyConfig) -> anyhow::Result<()>
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
                let target_addr = target_addr.clone();
                let proxy = proxy.clone();
                tokio::spawn(async move { relay(conn, target_addr, proxy).await });
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

async fn relay<'a, T>(
    mut inbound: TcpStream,
    target_addr: T,
    proxy: Arc<ProxyConfig>,
) -> anyhow::Result<()>
where
    T: IntoTargetAddr<'a> + Clone,
{
    let proxy_stream = TcpStream::connect(&proxy.address).await?;
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
