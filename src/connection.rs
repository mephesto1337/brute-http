use crate::Result;

use std::io;
use std::pin::Pin;
use std::task::{Context, Poll};

use tokio::io::{AsyncRead, AsyncWrite};

/// A generic connection
pub enum Connection {
    /// Plain text stream
    Plain(tokio::net::TcpStream),

    /// Encrypted stream
    Tls(async_native_tls::TlsStream<tokio::net::TcpStream>),
}

impl Connection {
    pub async fn new(remote: &str, use_tls: bool) -> Result<Self> {
        let stream = tokio::net::TcpStream::connect(remote).await?;
        if use_tls {
            let tls_stream = async_native_tls::TlsConnector::new()
                .danger_accept_invalid_hostnames(true)
                .connect(remote, stream)
                .await?;
            Ok(Self::Tls(tls_stream))
        } else {
            Ok(Self::Plain(stream))
        }
    }
}

impl AsyncRead for Connection {
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        match &mut *self {
            Self::Plain(ref mut stream) => AsyncRead::poll_read(Pin::new(stream), cx, buf),
            Self::Tls(ref mut stream) => AsyncRead::poll_read(Pin::new(stream), cx, buf),
        }
    }
}

impl AsyncWrite for Connection {
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        match &mut *self {
            Self::Plain(ref mut stream) => AsyncWrite::poll_write(Pin::new(stream), cx, buf),
            Self::Tls(ref mut stream) => AsyncWrite::poll_write(Pin::new(stream), cx, buf),
        }
    }

    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut *self {
            Self::Plain(ref mut stream) => AsyncWrite::poll_flush(Pin::new(stream), cx),
            Self::Tls(ref mut stream) => AsyncWrite::poll_flush(Pin::new(stream), cx),
        }
    }

    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        match &mut *self {
            Self::Plain(ref mut stream) => AsyncWrite::poll_shutdown(Pin::new(stream), cx),
            Self::Tls(ref mut stream) => AsyncWrite::poll_shutdown(Pin::new(stream), cx),
        }
    }
}

impl From<tokio::net::TcpStream> for Connection {
    fn from(s: tokio::net::TcpStream) -> Self {
        Self::Plain(s)
    }
}

impl From<async_native_tls::TlsStream<tokio::net::TcpStream>> for Connection {
    fn from(s: async_native_tls::TlsStream<tokio::net::TcpStream>) -> Self {
        Self::Tls(s)
    }
}
