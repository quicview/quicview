use clap::Parser;
use proto::{hmac_nonce_hex, parse_host_port, HELLO_PFX, OK};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};

#[derive(Debug, Parser)]
#[command(author, version, about = "QuicView minimal server (DLNK/1)")]
struct Args {
    /// Listen address for handshake server (e.g., 0.0.0.0:21116)
    #[arg(long, default_value = "0.0.0.0:21116")]
    listen: String,

    /// Optional HMAC key; if set, clients must send auth matching HMAC(nonce)
    #[arg(long)]
    key: Option<String>,

    /// Health endpoint listen address (TCP). If provided, accepts connections and closes.
    #[arg(long)]
    health: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    let (h, p) = parse_host_port(&args.listen, 21116);
    let listener = TcpListener::bind(format!("{h}:{p}")).await?;
    eprintln!("DLNK/1 listening on {h}:{p}");

    if let Some(health) = args.health.clone() {
        tokio::spawn(async move {
            if let Err(e) = run_health(health).await {
                eprintln!("health error: {e}");
            }
        });
    }

    loop {
        let (mut sock, peer) = listener.accept().await?;
        let key = args.key.clone();
        tokio::spawn(async move {
            if let Err(e) = handle_client(&mut sock, key.as_deref()).await {
                eprintln!("peer {peer} error: {e}");
            } else {
                eprintln!("peer {peer} handled");
            }
        });
    }
}

async fn run_health(addr: String) -> anyhow::Result<()> {
    let listener = TcpListener::bind(addr).await?;
    loop {
        let (mut sock, _) = listener.accept().await?;
        let _ = sock.shutdown().await;
    }
}

async fn handle_client(sock: &mut TcpStream, key: Option<&str>) -> anyhow::Result<()> {
    let mut buf = vec![0u8; 1024];
    let n = sock.read(&mut buf).await?;
    if n == 0 {
        anyhow::bail!("eof");
    }
    let line = String::from_utf8_lossy(&buf[..n]);
    if !line.starts_with(HELLO_PFX) {
        anyhow::bail!("bad prefix");
    }

    let mut nonce: Option<String> = None;
    let mut auth: Option<String> = None;
    for part in line.split_whitespace() {
        if let Some(v) = part.strip_prefix("nonce=") {
            nonce = Some(v.trim_end_matches('\n').to_string());
        }
        if let Some(v) = part.strip_prefix("auth=") {
            auth = Some(v.trim_end_matches('\n').to_string());
        }
    }

    let ok = match (nonce, key) {
        (Some(nonce_hex), Some(k)) => {
            let expected = hmac_nonce_hex(&nonce_hex, k.as_bytes());
            Some(expected) == auth
        }
        (Some(_), None) => true,
        _ => false,
    };

    if ok {
        sock.write_all(OK).await?;
    }
    Ok(())
}
