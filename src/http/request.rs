use std::fmt;

use nom::bytes::streaming::{tag, take, take_while, take_while1};
use nom::combinator::opt;
use nom::error::{context, ContextError, ParseError};
use nom::multi::many0;
use nom::sequence::{preceded, terminated, tuple};

use super::{get_body_size, is_chunked, retrieve_chunked_encoded_body, Header};
use crate::utils::{ascii_string, consume_spaces, crlf, parse_version};

#[derive(Debug, Eq, PartialEq)]
pub struct Request<'a> {
    /// HTTP method
    pub method: &'a str,

    /// Raw path up to the first '?' or '#'
    pub raw_path: &'a str,

    /// variables (from '?' to the end or '#')
    pub raw_variables: Vec<(&'a str, &'a str)>,

    /// anchor
    pub raw_anchor: Option<&'a str>,

    /// HTTP version used
    pub version: (u8, u8),

    /// HTTP request headers
    headers: Vec<Header<'a>>,

    /// Body
    pub body: &'a [u8],
}

impl<'a> Request<'a> {
    pub fn parse<E>(input: &'a [u8]) -> nom::IResult<&'a [u8], Self, E>
    where
        E: ParseError<&'a [u8]> + ContextError<&'a [u8]>,
    {
        let (rest, (method, raw_path, raw_variables, raw_anchor, version)) = context(
            "HTTP request first line",
            tuple((
                ascii_string(take_while1(nom::character::is_alphabetic)),
                preceded(
                    consume_spaces,
                    ascii_string(take_while1(|b: u8| {
                        b.is_ascii() && !(b.is_ascii_whitespace() || b == b'?' || b == b'#')
                    })),
                ),
                opt(preceded(
                    tag(&b"?"[..]),
                    ascii_string(take_while(|b: u8| {
                        b.is_ascii() && !(b.is_ascii_whitespace() || b == b'#')
                    })),
                )),
                opt(preceded(
                    tag(&b"#"[..]),
                    ascii_string(take_while(|b: u8| b.is_ascii() && !b.is_ascii_whitespace())),
                )),
                preceded(
                    consume_spaces,
                    terminated(preceded(tag(&b"HTTP/"[..]), parse_version), crlf),
                ),
            )),
        )(input)?;
        eprintln!("Got first line");

        let raw_variables = if let Some(vars) = raw_variables {
            vars.split('&')
                .map(|key_value: &str| {
                    if let Some(kv) = key_value.split_once('=') {
                        kv
                    } else {
                        (key_value, "")
                    }
                })
                .collect()
        } else {
            Vec::new()
        };

        let (rest, headers) = context("HTTP headers", many0(Header::parse))(rest)?;
        let (rest, _) = context("HTTP headers end", crlf)(rest)?;
        if let Some(body_length) = get_body_size(&headers[..]) {
            let (rest, body) = context("HTTP body", take(body_length))(rest)?;
            Ok((
                rest,
                Self {
                    method,
                    raw_path,
                    raw_variables,
                    raw_anchor,
                    version,
                    headers,
                    body,
                },
            ))
        } else if is_chunked(&headers[..]) {
            let (rest, body) = retrieve_chunked_encoded_body(rest)?;
            Ok((
                rest,
                Self {
                    method,
                    raw_path,
                    raw_variables,
                    raw_anchor,
                    version,
                    headers,
                    body,
                },
            ))
        } else {
            let body = &b""[..];
            Ok((
                rest,
                Self {
                    method,
                    raw_path,
                    raw_variables,
                    raw_anchor,
                    version,
                    headers,
                    body,
                },
            ))
        }
    }

    pub fn path(&self) -> String {
        self.raw_path.into()
    }

    pub fn variables(&self) -> Vec<(String, String)> {
        self.raw_variables
            .iter()
            .map(|&(k, v)| (k.into(), v.into()))
            .collect()
    }

    pub fn has_variables(&self) -> bool {
        !self.raw_variables.is_empty()
    }

    pub fn anchor(&self) -> Option<String> {
        self.raw_anchor.map(String::from)
    }

    pub fn headers(&self) -> &[Header<'a>] {
        &self.headers[..]
    }
}

impl fmt::Display for Request<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} {}", self.method, self.path())?;
        if self.has_variables() {
            f.write_str("?")?;
            for (key, value) in self.variables() {
                write!(f, "{}={}", key, value)?;
            }
        }
        if let Some(anchor) = self.anchor() {
            write!(f, "#{}", anchor)?;
        }
        write!(f, " HTTP/{}.{}\r\n", self.version.0, self.version.1)?;
        for header in self.headers() {
            write!(f, "{}", header)?;
        }

        f.write_str("\r\n")
    }
}

#[cfg(test)]
mod tests {
    use crate::utils::hex::Hex;
    use crate::Error;

    use super::*;

    #[test]
    fn parse_http_request() {
        let request = b"\
        GET /path?var1=value1&var2=&var1#anchor HTTP/1.1\r\n\
        Host: localhost\r\n\
        Connection: Closed\r\n\
        \r\n\
        extra data";

        let maybe_request = Request::parse::<nom::error::VerboseError<&[u8]>>(&request[..])
            .map_err(|e| Error::from(e).map_input(Hex::from));

        assert_eq!(
            maybe_request,
            Ok((
                &b"extra data"[..],
                Request {
                    method: "GET",
                    raw_path: "/path",
                    raw_variables: vec![("var1", "value1"), ("var2", ""), ("var3", ""),],
                    raw_anchor: Some("anchor"),
                    version: (1, 1),
                    headers: vec![
                        Header {
                            name: "Host",
                            value: "localhost"
                        },
                        Header {
                            name: "Connection",
                            value: "Closed"
                        },
                    ],
                    body: &b""[..]
                }
            ))
        );
    }
}
