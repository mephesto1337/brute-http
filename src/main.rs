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
    #[clap(short, long)]
    request: PathBuf,

    /// Remote destination HOST:PORT
    #[clap(parse(try_from_str))]
    target: String,

    /// Number of threads to use
    #[clap(short, long, parse(try_from_str))]
    threads: Option<usize>,

    /// Only test request and dumps response
    #[clap(long)]
    test: bool,

    /// Use SSL
    #[clap(short, long)]
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
        format!("u64 overflow \\o/ !")
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
    let args = Options::parse();
    let request = tokio::fs::read(&args.request).await?.leak();
    let request = &*request;
    let threads_count = args.threads.unwrap_or(get_cpu_count().await?);

    if args.test {
        let mut stream = Connection::new(&args.target, args.use_tls).await?;
        let mut buffer = Vec::with_capacity(8192);
        send_request(&mut stream, request, &mut buffer).await?;
        let (rest, response) =
            http::Response::parse::<nom::error::VerboseError<_>>(&buffer[..]).unwrap();
        println!("{}", response);
        if !rest.is_empty() {
            eprintln!("Got extra bytes: {:#?}", rest);
        }
        return Ok(());
    }

    let target = &*Box::leak(args.target.into_boxed_str());
    let mut tasks: Vec<_> = (0..threads_count)
        .map(|i| {
            tokio::spawn(async move {
                eprintln!("Starting task {}", i);
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
            eprintln!("Issue with task {}: {}", i, e);
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
                    eprintln!("Could not parse response");
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
                eprintln!("Cannot connect to {}: {:?}", remote, e);
                return;
            }
        };

        if let Err(e) = send_requests(&mut stream, request).await {
            eprintln!("Error while sending request to {}: {:?}", remote, e);
        }
    }
}