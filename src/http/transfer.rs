use std::fmt;

use nom::bytes::streaming::{tag, take};
use nom::error::{context, ContextError, ParseError};
use nom::sequence::terminated;

use crate::http::{get_body_size, Header};
use crate::utils::hex::Hex;
use crate::utils::{crlf, parse_usize_hex};

/// Transfer Encoding for HTTP bodies
#[derive(Eq)]
pub enum TransferEncoding<'a> {
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

pub struct TransferEncodingIter<'a, 'b> {
    index: usize,
    te: &'b TransferEncoding<'a>,
}

impl<'a, 'b> Iterator for TransferEncodingIter<'a, 'b>
where
    'b: 'a,
{
    type Item = &'a [u8];

    fn next(&mut self) -> Option<Self::Item> {
        let chunk = match self.te {
            TransferEncoding::Regular(body) => std::iter::once(body).skip(self.index).next(),
            TransferEncoding::Chunked(chunks) => chunks.get(self.index),
            TransferEncoding::Compress(body) => std::iter::once(body).skip(self.index).next(),
            TransferEncoding::Deflate(body) => std::iter::once(body).skip(self.index).next(),
            TransferEncoding::Gzip(body) => std::iter::once(body).skip(self.index).next(),
        };
        self.index += 1;
        chunk.map(|&x| x)
    }
}

impl<'a> TransferEncoding<'a> {
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

    pub fn iter<'b>(&'b self) -> TransferEncodingIter<'a, 'b> {
        TransferEncodingIter { index: 0, te: self }
    }

    fn is_equal(&'a self, other: &Self) -> Option<()> {
        let mut chunks_self = self.iter();
        let mut chunks_other = other.iter();

        let (mut chunk_self, mut chunk_other) = match (chunks_self.next(), chunks_other.next()) {
            (Some(x), Some(y)) => (x, y),
            (None, None) => return Some(()),
            _ => return None,
        };

        loop {
            let size = chunk_self.len().min(chunk_other.len());
            if &chunk_self[..size] != &chunk_other[..size] {
                return None;
            }
            chunk_self = &chunk_self[size..];
            chunk_other = &chunk_other[size..];
            match (chunk_self.len(), chunk_other.len()) {
                (0, 0) => match (chunks_self.next(), chunks_other.next()) {
                    (Some(x), Some(y)) => {
                        (chunk_self, chunk_other) = (x, y);
                    }
                    (None, None) => return Some(()),
                    _ => return None,
                },
                (0, _) => {
                    chunk_self = chunks_self.next()?;
                }
                (_, 0) => {
                    chunk_other = chunks_other.next()?;
                }
                (_, _) => {
                    unreachable!()
                }
            }
        }
    }
}

impl fmt::Debug for TransferEncoding<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Regular(c) => f.debug_tuple("Regular").field(&Hex::from(*c)).finish(),
            Self::Chunked(c) => f.debug_tuple("Chunked").field(&Hex::from(c[0])).finish(),
            Self::Compress(c) => f.debug_tuple("Compress").field(&Hex::from(*c)).finish(),
            Self::Deflate(c) => f.debug_tuple("Deflate").field(&Hex::from(*c)).finish(),
            Self::Gzip(c) => f.debug_tuple("Gzip").field(&Hex::from(*c)).finish(),
        }
    }
}

impl std::cmp::PartialEq for TransferEncoding<'_> {
    fn eq(&self, other: &Self) -> bool {
        if self.is_equal(other).is_some() {
            eprintln!("{:?} == {:?}", self, other);
            true
        } else {
            eprintln!("{:?} != {:?}", self, other);
            false
        }
    }
}
