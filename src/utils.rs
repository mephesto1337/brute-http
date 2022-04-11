use nom::bytes::streaming::{tag, take_while1};
use nom::combinator::{map, map_opt, verify};
use nom::error::{context, ContextError, ParseError};
use nom::sequence::tuple;

pub mod hex;

macro_rules! def_parse_integer {
    ($name:ident, $name_hex:ident, $int_type:ty) => {
        #[allow(dead_code)]
        pub fn $name<'a, E>(input: &'a [u8]) -> nom::IResult<&'a [u8], $int_type, E>
        where
            E: ParseError<&'a [u8]>,
        {
            map_opt(ascii_string(take_while1(nom::character::is_digit)), |s| {
                s.parse::<$int_type>().ok()
            })(input)
        }

        #[allow(dead_code)]
        pub fn $name_hex<'a, E>(input: &'a [u8]) -> nom::IResult<&'a [u8], $int_type, E>
        where
            E: ParseError<&'a [u8]>,
        {
            map_opt(
                ascii_string(take_while1(nom::character::is_hex_digit)),
                |s| <$int_type>::from_str_radix(s, 16).ok(),
            )(input)
        }
    };
}

def_parse_integer!(parse_u8, parse_u8_hex, u8);
def_parse_integer!(parse_u16, parse_u16_hex, u16);
def_parse_integer!(parse_u32, parse_u32_hex, u32);
def_parse_integer!(parse_u64, parse_u64_hex, u64);
def_parse_integer!(parse_usize, parse_usize_hex, usize);

pub fn consume_spaces<'a, E>(input: &'a [u8]) -> nom::IResult<&'a [u8], (), E>
where
    E: ParseError<&'a [u8]> + ContextError<&'a [u8]>,
{
    context("ascii spaces", map(take_while1(|b| b == b' '), |_| ()))(input)
}

pub fn crlf<'a, E>(input: &'a [u8]) -> nom::IResult<&'a [u8], (), E>
where
    E: ParseError<&'a [u8]> + ContextError<&'a [u8]>,
{
    context("CRLF", map(tag(&b"\r\n"[..]), |_| ()))(input)
}

pub fn ascii_string<'a, E, F>(
    mut f: F,
) -> impl FnMut(&'a [u8]) -> nom::IResult<&'a [u8], &'a str, E>
where
    E: ParseError<&'a [u8]>,
    F: nom::Parser<&'a [u8], &'a [u8], E>,
{
    move |input: &[u8]| {
        map(
            verify(|i| f.parse(i), |b: &[u8]| b.is_ascii()),
            |b: &[u8]| unsafe { std::str::from_utf8_unchecked(b) },
        )(input)
    }
}

pub fn parse_version<'a, E>(input: &'a [u8]) -> nom::IResult<&'a [u8], (u8, u8), E>
where
    E: ParseError<&'a [u8]> + ContextError<&'a [u8]>,
{
    let (rest, (major, _dot, minor)) =
        context("HTTP version", tuple((parse_u8, tag(&b"."[..]), parse_u8)))(input)?;

    Ok((rest, (major, minor)))
}
