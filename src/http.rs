use std::fmt;

use nom::bytes::streaming::{tag, take, take_while1};
use nom::combinator::map;
use nom::error::{context, ContextError, ParseError};
use nom::sequence::{separated_pair, terminated, tuple};

mod response;
pub use response::Response;

mod request;
pub use request::Request;

use crate::utils::{ascii_string, consume_spaces, crlf, parse_usize_hex};

/// HTTP header
#[derive(Debug, Eq, PartialEq)]
pub struct Header<'a> {
    /// HTTP header name
    pub name: &'a str,

    /// HTTP header value (without \r\n)
    pub value: &'a str,
}

impl fmt::Display for Header<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}\r\n", self.name, self.value)
    }
}

impl<'a> Header<'a> {
    pub fn parse<E>(input: &'a [u8]) -> nom::IResult<&'a [u8], Self, E>
    where
        E: ParseError<&'a [u8]> + ContextError<&'a [u8]>,
    {
        context(
            "HTTP header",
            map(
                separated_pair(
                    ascii_string(take_while1(|b: u8| b.is_ascii_alphanumeric() || b == b'-')),
                    tuple((tag(&b":"[..]), consume_spaces)),
                    terminated(
                        ascii_string(take_while1(|b: u8| {
                            b.is_ascii_punctuation() || b == b' ' || b.is_ascii_alphanumeric()
                        })),
                        crlf,
                    ),
                ),
                |(name, value)| Self { name, value },
            ),
        )(input)
    }
}

pub fn get_body_size(headers: &[Header<'_>]) -> Option<usize> {
    headers.iter().find_map(|h| {
        if h.name.eq_ignore_ascii_case("Content-Length") {
            h.value.parse::<usize>().ok()
        } else {
            None
        }
    })
}

pub fn is_chunked(headers: &[Header<'_>]) -> bool {
    headers
        .iter()
        .find(|h| {
            h.name.eq_ignore_ascii_case("Transfer-Encoding")
                && h.value.eq_ignore_ascii_case("chunked")
        })
        .is_some()
}

pub fn retrieve_chunked_encoded_body<'a, E>(input: &'a [u8]) -> nom::IResult<&'a [u8], &'a [u8], E>
where
    E: ParseError<&'a [u8]> + ContextError<&'a [u8]>,
{
    let input_orig = input;
    let mut input = input;
    loop {
        let (rest, chunk_size) =
            context("HTTP Chunk size", terminated(parse_usize_hex, crlf))(input)?;
        input = rest;

        let (rest, _chunk) = context(
            "HTTP chunk data",
            terminated(take(chunk_size), tag(&b"\r\n"[..])),
        )(input)?;
        input = rest;

        if chunk_size == 0 {
            break;
        }
    }

    assert!(input_orig.len() > input.len());
    let offset = input_orig.len() - input.len();

    let body = &input_orig[..offset];
    let rest = &input_orig[offset..];
    Ok((rest, body))
}
