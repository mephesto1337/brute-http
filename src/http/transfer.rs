use std::borrow::Cow;
use std::fmt;
use std::io::{Read, Write};

use nom::bytes::streaming::{tag, take};
use nom::error::{context, ContextError, ParseError};
use nom::sequence::terminated;

use flate2::read::{GzDecoder, ZlibDecoder};

use crate::http::{get_body_size, Header};
use crate::utils::{crlf, parse_usize_hex};

/// Transfer Encoding for HTTP bodies
enum TransferEncodingInner<'a> {
    /// Just a "normal" body
    Regular(&'a [u8]),

    /// Data is sent in a series of chunks
    Chunked(Vec<&'a [u8]>),

    /// A format using the Lempel-Ziv-Welch (LZW) algorithm.
    Compress(&'a [u8]),

    /// Using the zlib structure (defined in RFC 1950), with the deflate compression algorithm
    /// (defined in RFC 1951).
    Deflate(&'a [u8]),

    /// A format using the Lempel-Ziv coding (LZ77), with a 32-bit CRC.
    Gzip(&'a [u8]),
}

impl<'a> TransferEncodingInner<'a> {
    fn parse_chunked<E>(input: &'a [u8]) -> nom::IResult<&'a [u8], Vec<&'a [u8]>, E>
    where
        E: ParseError<&'a [u8]> + ContextError<&'a [u8]>,
    {
        let mut unparsed = input;
        let mut chunks = Vec::new();
        loop {
            let (rest, chunk_size) =
                context("HTTP Chunk size", terminated(parse_usize_hex, crlf))(unparsed)?;
            unparsed = rest;

            let (rest, chunk) = context(
                "HTTP chunk data",
                terminated(take(chunk_size), tag(&b"\r\n"[..])),
            )(unparsed)?;
            chunks.push(chunk);
            unparsed = rest;

            if chunk_size == 0 {
                break;
            }
        }

        Ok((unparsed, chunks))
    }

    fn parse_content_length<E>(
        input: &'a [u8],
        content_length: usize,
    ) -> nom::IResult<&'a [u8], &'a [u8], E>
    where
        E: ParseError<&'a [u8]> + ContextError<&'a [u8]>,
    {
        context("HTTP Body wih Content-Length", take(content_length))(input)
    }

    pub fn parse<E>(input: &'a [u8], headers: &[Header<'_>]) -> nom::IResult<&'a [u8], Self, E>
    where
        E: ParseError<&'a [u8]> + ContextError<&'a [u8]>,
    {
        match (
            Header::get_value(headers, "Transfer-Encoding"),
            get_body_size(headers),
        ) {
            (Some("chunked"), None) => {
                let (rest, chunks) = Self::parse_chunked(input)?;
                Ok((rest, Self::Chunked(chunks)))
            }
            (Some("compress"), Some(size)) => {
                let (rest, body) = Self::parse_content_length(input, size)?;
                Ok((rest, Self::Compress(body)))
            }
            (Some("deflate"), Some(size)) => {
                let (rest, body) = Self::parse_content_length(input, size)?;
                Ok((rest, Self::Deflate(body)))
            }
            (Some("gzip"), Some(size)) => {
                let (rest, body) = Self::parse_content_length(input, size)?;
                Ok((rest, Self::Gzip(body)))
            }
            (Some(_), _) => Err(nom::Err::Failure(E::add_context(
                input,
                "Invalid Transfer Encoding/Content-Length",
                E::from_error_kind(input, nom::error::ErrorKind::Verify),
            ))),
            (None, Some(size)) => {
                let (rest, body) = Self::parse_content_length(input, size)?;
                Ok((rest, Self::Regular(body)))
            }
            (None, None) => {
                // Err(nom::Err::Failure(E::add_context(
                //     input,
                //     "No Transfer Encoding or Content-Length",
                //     E::from_error_kind(input, nom::error::ErrorKind::NoneOf),
                // )))
                Ok((input, Self::Regular(&b""[..])))
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum TransferEncodingKind {
    /// Just a "normal" body
    Regular,

    /// Data is sent in a series of chunks
    Chunked,

    /// A format using the Lempel-Ziv-Welch (LZW) algorithm.
    Compress,

    /// Using the zlib structure (defined in RFC 1950), with the deflate compression algorithm
    /// (defined in RFC 1951).
    Deflate,

    /// A format using the Lempel-Ziv coding (LZ77), with a 32-bit CRC.
    Gzip,
}

#[derive(Eq, PartialEq)]
pub struct Body<'a> {
    /// The kind being used
    pub kind: TransferEncodingKind,

    /// The decoded content
    pub content: Cow<'a, [u8]>,
}

impl<'a> From<&'a [u8]> for Body<'a> {
    fn from(value: &'a [u8]) -> Self {
        Self {
            kind: TransferEncodingKind::Regular,
            content: Cow::Borrowed(value),
        }
    }
}

impl<'a> From<Vec<u8>> for Body<'a> {
    fn from(value: Vec<u8>) -> Self {
        Self {
            kind: TransferEncodingKind::Regular,
            content: Cow::Owned(value),
        }
    }
}

impl<'a> fmt::Debug for Body<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Body")
            .field("kind", &self.kind)
            .field("length", &self.content.len())
            .finish()
    }
}

impl<'a> Body<'a> {
    pub fn parse<E>(input: &'a [u8], headers: &[Header<'_>]) -> nom::IResult<&'a [u8], Self, E>
    where
        E: ParseError<&'a [u8]> + ContextError<&'a [u8]>,
    {
        let (rest, te) = TransferEncodingInner::parse(input, headers)?;

        let body = match te {
            TransferEncodingInner::Regular(content) => Self {
                kind: TransferEncodingKind::Regular,
                content: Cow::Borrowed(content),
            },
            TransferEncodingInner::Chunked(chunks) => {
                let mut content = Vec::with_capacity(chunks.iter().map(|c| c.len()).sum());
                for chunk in chunks {
                    content
                        .write_all(chunk)
                        .expect("Writing into a Vec should not fail");
                }
                Self {
                    kind: TransferEncodingKind::Chunked,
                    content: Cow::Owned(content),
                }
            }
            TransferEncodingInner::Gzip(gzip) => {
                let mut content = Vec::with_capacity(gzip.len());
                let mut decoder = GzDecoder::new(gzip);
                match decoder.read_to_end(&mut content) {
                    Ok(_) => Self {
                        kind: TransferEncodingKind::Gzip,
                        content: Cow::Owned(content),
                    },
                    Err(_) => {
                        return Err(nom::Err::Failure(E::add_context(
                            input,
                            "Invalid gzip content",
                            E::from_error_kind(input, nom::error::ErrorKind::Verify),
                        )));
                    }
                }
            }
            TransferEncodingInner::Deflate(zlib) => {
                let mut content = Vec::with_capacity(zlib.len());
                let mut decoder = ZlibDecoder::new(zlib);
                match decoder.read_to_end(&mut content) {
                    Err(_) => {
                        return Err(nom::Err::Failure(E::add_context(
                            input,
                            "Invalid zlib content",
                            E::from_error_kind(input, nom::error::ErrorKind::Verify),
                        )));
                    }
                    Ok(_) => Self {
                        kind: TransferEncodingKind::Deflate,
                        content: Cow::Owned(content),
                    },
                }
            }
            TransferEncodingInner::Compress(_) => {
                return Err(nom::Err::Failure(E::add_context(
                    input,
                    "LZW/Compress is not handled",
                    E::from_error_kind(input, nom::error::ErrorKind::NoneOf),
                )));
            }
        };

        Ok((rest, body))
    }
}
