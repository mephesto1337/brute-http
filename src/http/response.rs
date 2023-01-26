use std::fmt;

use nom::bytes::streaming::{tag, take_until};
use nom::combinator::verify;
use nom::error::{context, ContextError, ParseError};
use nom::multi::many1;
use nom::sequence::{preceded, terminated, tuple};

use crate::http::{Body, Header};
use crate::utils::{ascii_string, consume_spaces, crlf, parse_u16, parse_version};

/// HTTP Response
#[derive(Debug, Eq, PartialEq)]
pub struct Response<'a> {
    /// Version used by the server
    pub version: (u8, u8),

    /// Response's code
    pub code: u16,

    /// Message associated with code
    pub message: &'a str,

    /// headers,
    headers: Vec<Header<'a>>,

    /// body
    pub body: Body,
}

impl<'a> Response<'a> {
    pub fn headers(&self) -> &[Header<'a>] {
        &self.headers[..]
    }

    pub fn parse<E>(input: &'a [u8]) -> nom::IResult<&'a [u8], Self, E>
    where
        E: ParseError<&'a [u8]> + ContextError<&'a [u8]>,
    {
        let (rest, (version, code, message)) = context(
            "HTTP response first line",
            tuple((
                preceded(tag(&b"HTTP/"[..]), parse_version),
                preceded(
                    consume_spaces,
                    context(
                        "HTTP status code",
                        verify(parse_u16, |c| 100 <= *c && *c <= 599),
                    ),
                ),
                ascii_string(preceded(
                    consume_spaces,
                    terminated(take_until(&b"\r\n"[..]), crlf),
                )),
            )),
        )(input)?;

        let (rest, headers) = context("HTTP headers", many1(Header::parse))(rest)?;
        let (rest, _) = context("HTTP headers end", crlf)(rest)?;
        let (rest, body) = Body::parse(rest, &headers[..])?;
        Ok((
            rest,
            Self {
                version,
                code,
                message,
                headers,
                body,
            },
        ))
    }
}

impl fmt::Display for Response<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "HTTP/{}.{} {} {}\r\n",
            self.version.0, self.version.1, self.code, self.message
        )?;
        for header in self.headers() {
            write!(f, "{}", header)?;
        }

        f.write_str("\r\n")
    }
}

#[cfg(test)]
mod tests {
    use crate::http::TransferEncodingKind;
    use crate::utils::hex::Hex;
    use crate::Error;

    use super::*;

    #[test]
    fn parse_http_response() {
        let response = b"\
        HTTP/1.1 200 Ok\r\n\
        Server: Test Server 0.0.1\r\n\
        Content-Length: 12\r\n\
        Content-Type: text/plain\r\n\
        Connection: Closed\r\n\
        \r\n\
        hello world!\
        extra data";

        let maybe_response = Response::parse::<nom::error::VerboseError<&[u8]>>(&response[..])
            .map_err(|e| Error::from(e).map_input(Hex::from));

        assert_eq!(
            maybe_response,
            Ok((
                &b"extra data"[..],
                Response {
                    version: (1, 1),
                    code: 200,
                    message: "Ok",
                    headers: vec![
                        Header {
                            name: "Server",
                            value: "Test Server 0.0.1"
                        },
                        Header {
                            name: "Content-Length",
                            value: "12"
                        },
                        Header {
                            name: "Content-Type",
                            value: "text/plain"
                        },
                        Header {
                            name: "Connection",
                            value: "Closed"
                        }
                    ],
                    body: Body {
                        kind: TransferEncodingKind::Regular,
                        content: b"hello world!"[..].to_vec()
                    }
                }
            )),
            "Bad response: {:#?}",
            maybe_response
        );
    }

    #[test]
    fn parse_http_chunked() {
        let response = b"\
        HTTP/1.1 400 Bad Request\r\n\
        Server: nginx\r\n\
        Date: Thu, 07 Apr 2022 14:20:20 GMT\r\n\
        Content-Type: text/html; charset=UTF-8\r\n\
        Transfer-Encoding: chunked\r\n\
        Connection: keep-alive\r\n\
        \r\n\
        10\r\n\
        0123456789abcdef\r\n\
        0\r\n\
        \r\n
        ";
        eprintln!(
            "{:x?}",
            Response::parse::<nom::error::VerboseError<&[u8]>>(&response[..])
        );
    }
}
