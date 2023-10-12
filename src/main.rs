use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::time::sleep;

use clap::Parser;

mod connection;
pub mod error;
pub mod http;
pub(crate) mod utils;

use connection::Connection;
pub use error::{Error, Result};

#[derive(Debug, Parser)]
#[clap(author, version, about, long_about = None)]
struct Options {
    /// Input file for HTTP request
    #[arg(short, long)]
    request: PathBuf,

    /// Remote destination HOST:PORT
    target: String,

    /// Number of tasks to use (default 10/core)
    #[arg(short, long)]
    tasks: Option<usize>,

    /// Only test request and dumps response
    #[arg(long)]
    test: bool,

    /// Use SSL
    #[arg(short, long)]
    use_tls: bool,
}

fn format_bandwidth(bytes: u64, seconds: u64) -> String {
    const KILO: f64 = 1024f64;
    const MEGA: f64 = KILO * 1024f64;
    const GIGA: f64 = MEGA * 1024f64;

    if let Some(bits) = bytes.checked_mul(8) {
        let bandwitdh = bits as f64 / seconds as f64;
        if bandwitdh < KILO {
            format!("{:>7}  bps", bandwitdh)
        } else if bandwitdh < MEGA {
            format!("{:>8.3} Kbps", bandwitdh / KILO)
        } else if bandwitdh < GIGA {
            format!("{:>8.3} Mbps", bandwitdh / MEGA)
        } else {
            format!("{:>8.3} Gbps", bandwitdh / GIGA)
        }
    } else {
        "u64 overflow \\o/ !".to_owned()
    }
}

async fn get_cpu_count() -> Result<usize> {
    let cpuinfo = tokio::fs::read_to_string("/proc/cpuinfo").await?;
    Ok(cpuinfo
        .lines()
        .filter(|s| s.starts_with("processor\t"))
        .count())
}

static BYTES_SEND: AtomicU64 = AtomicU64::new(0);
static BYTES_RECV: AtomicU64 = AtomicU64::new(0);
static RESPONSE_TIME: AtomicU64 = AtomicU64::new(0);
static RESPONSE_COUNT: AtomicU64 = AtomicU64::new(0);

#[tokio::main]
async fn main() -> Result<()> {
    env_logger::init();

    let args = Options::parse();
    let request = tokio::fs::read(&args.request).await?.leak();
    let request = &*request;
    match http::Request::parse::<()>(request) {
        Ok((rest, req)) => {
            if !rest.is_empty() {
                let s: utils::hex::Hex = rest.into();
                log::warn!("There is remaining bytes in the request that may not be handled by the server: {s:?}");
            }
            match req.version {
                (0, 9) | (1, 0) | (1, 1) => {}
                (a, b) => log::error!("Unsupported HTTP version: {a}.{b}"),
            }
        }
        Err(e) => {
            log::error!("Could not parse request: {e:?}");
        }
    }
    let tasks_count = args.tasks.unwrap_or(get_cpu_count().await? * 10);

    if args.test {
        let mut stream = Connection::new(&args.target, args.use_tls).await?;
        let mut buffer = Vec::with_capacity(8192);
        send_request(&mut stream, request, &mut buffer).await?;
        let (rest, response) =
            http::Response::parse::<nom::error::VerboseError<_>>(&buffer[..]).unwrap();
        println!("{:?}", response);
        if !rest.is_empty() {
            log::warn!("Got extra bytes: {:#?}", rest);
        }
        return Ok(());
    }

    let target = &*Box::leak(args.target.into_boxed_str());
    let mut tasks: Vec<_> = (0..tasks_count)
        .map(|i| {
            tokio::spawn(async move {
                log::debug!("Starting task {}", i);
                brute_server(target, request, args.use_tls).await;
            })
        })
        .collect();

    tasks.push(tokio::spawn(async {
        loop {
            sleep(Duration::from_secs(2)).await;
            let up = BYTES_SEND.swap(0, Ordering::Relaxed);
            let down = BYTES_RECV.swap(0, Ordering::Relaxed);
            let response_time = RESPONSE_TIME.swap(0, Ordering::Relaxed);
            let response_count = RESPONSE_COUNT.swap(0, Ordering::Relaxed);

            println!(
                "Up {:12} | Down {:12} | {:>8.3} msec/response",
                format_bandwidth(up, 1),
                format_bandwidth(down, 1),
                response_time as f64 / response_count as f64
            );
        }
    }));

    for (i, t) in tasks.iter_mut().enumerate() {
        if let Err(e) = t.await {
            log::error!("Issue with task {}: {}", i, e);
        }
    }

    Ok(())
}

async fn send_request<S>(
    stream: &mut S,
    request: &[u8],
    response_buffer: &mut Vec<u8>,
) -> Result<()>
where
    S: AsyncReadExt + AsyncWriteExt + Unpin + Send + Sync,
{
    stream.write_all(request).await?;
    let now = Instant::now();
    BYTES_SEND.fetch_add(request.len() as u64, Ordering::Relaxed);
    response_buffer.clear();

    loop {
        let n = stream.read_buf(response_buffer).await?;
        if n == 0 {
            // Reached EOF
            return Err(std::io::Error::new(
                std::io::ErrorKind::UnexpectedEof,
                "Could not received response",
            )
            .into());
        }
        BYTES_RECV.fetch_add(n as u64, Ordering::Relaxed);

        match http::Response::parse(&response_buffer[..]) {
            Ok(_) => {
                if let Ok(elaped) = now.elapsed().as_millis().try_into() {
                    RESPONSE_TIME.fetch_add(elaped, Ordering::Relaxed);
                    RESPONSE_COUNT.fetch_add(1, Ordering::Relaxed);
                }
                return Ok(());
            }
            Err(e) => {
                if !e.is_incomplete() {
                    log::error!("Could not parse response");
                    return Err(e.into());
                }
            }
        }
    }
}

async fn send_requests<S>(stream: &mut S, request: &[u8]) -> Result<()>
where
    S: AsyncReadExt + AsyncWriteExt + Unpin + Send + Sync,
{
    let mut response_buffer = Vec::with_capacity(8192);
    loop {
        send_request(stream, request, &mut response_buffer).await?;
    }
}

async fn brute_server(remote: &str, request: &[u8], use_tls: bool) {
    loop {
        let mut stream = match Connection::new(remote, use_tls).await {
            Ok(s) => s,
            Err(e) => {
                log::error!("Cannot connect to {}: {:?}", remote, e);
                return;
            }
        };

        if let Err(e) = send_requests(&mut stream, request).await {
            log::error!("Error while sending request to {}: {:?}", remote, e);
        }
    }
}
