use std::fmt;

use nom::bytes::streaming::{tag, take_while1};
use nom::combinator::map;
use nom::error::{context, ContextError, ParseError};
use nom::sequence::{separated_pair, terminated, tuple};

mod transfer;
pub use transfer::{Body, TransferEncodingKind};

mod response;
pub use response::Response;

mod request;
pub use request::Request;

use crate::utils::{ascii_string, consume_spaces, crlf};

/// HTTP header
#[derive(Eq)]
pub struct Header<'a> {
    /// HTTP header name
    pub name: &'a str,

    /// HTTP header value (without \r\n)
    pub value: &'a str,
}

impl fmt::Debug for Header<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "{}: {}\\r\\n", self.name, self.value)
    }
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

    pub fn get_value(headers: &[Header<'a>], needle: &str) -> Option<&'a str> {
        Self::get_values(headers, needle).next()
    }

    pub fn get_values<'b>(
        headers: &'b [Header<'a>],
        needle: &'b str,
    ) -> impl Iterator<Item = &'a str> + 'b
    where
        'a: 'b,
    {
        headers
            .iter()
            .filter_map(|h| h.name.eq_ignore_ascii_case(needle).then_some(h.value))
    }
}

pub fn get_body_size(headers: &[Header<'_>]) -> Option<usize> {
    Header::get_value(headers, "Content-Length").and_then(|v| v.parse::<usize>().ok())
}

impl<'a> std::cmp::PartialEq for Header<'a> {
    fn eq(&self, other: &Self) -> bool {
        self.name.eq_ignore_ascii_case(other.name) && self.value == other.value
    }
}
