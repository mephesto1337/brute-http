use std::fmt;
use std::io;

use crate::utils::hex::Hex;

pub type Result<T, E = nom::error::Error<Vec<u8>>> = std::result::Result<T, Error<E>>;

/// Errors for this crate
#[derive(Debug)]
pub enum Error<E> {
    /// Underlying I/O Error
    IO(io::Error),

    /// Issue when parsing HTTP
    Parse(nom::Err<E>),

    /// TLS error
    TLS(async_native_tls::Error),
}

impl<E> fmt::Display for Error<E>
where
    E: fmt::Display + fmt::Debug,
{
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::IO(ref e) => fmt::Display::fmt(e, f),
            Self::Parse(ref e) => fmt::Display::fmt(e, f),
            Self::TLS(ref e) => fmt::Display::fmt(e, f),
        }
    }
}

impl<E> From<io::Error> for Error<E> {
    fn from(e: io::Error) -> Self {
        Self::IO(e)
    }
}

impl<I> From<nom::Err<nom::error::Error<I>>> for Error<nom::error::Error<Vec<u8>>>
where
    I: AsRef<[u8]>,
{
    fn from(e: nom::Err<nom::error::Error<I>>) -> Self {
        Error::Parse(e.map_input(|i| i.as_ref().to_vec()))
    }
}

impl<I> From<nom::Err<nom::error::Error<I>>> for Error<nom::error::Error<Hex<'_>>>
where
    I: AsRef<[u8]>,
{
    fn from(e: nom::Err<nom::error::Error<I>>) -> Self {
        Self::Parse(e.map_input(|i| Hex::from(i.as_ref().to_vec())))
    }
}

impl<I> From<nom::Err<nom::error::VerboseError<I>>> for Error<nom::error::VerboseError<Hex<'_>>>
where
    I: AsRef<[u8]>,
{
    fn from(e: nom::Err<nom::error::VerboseError<I>>) -> Self {
        Error::Parse(e).map_input(|i| Hex::from(i.as_ref().to_vec()))
    }
}

impl<E> From<async_native_tls::Error> for Error<E> {
    fn from(e: async_native_tls::Error) -> Self {
        Self::TLS(e)
    }
}

impl<I, E> nom::error::ParseError<I> for Error<E>
where
    E: nom::error::ParseError<I>,
{
    fn from_error_kind(i: I, e: nom::error::ErrorKind) -> Self {
        Self::Parse(nom::Err::Error(E::from_error_kind(i, e)))
    }
    fn append(i: I, ek: nom::error::ErrorKind, e: Self) -> Self {
        match e {
            Self::Parse(nom::Err::Error(e)) => Self::Parse(nom::Err::Error(E::append(i, ek, e))),
            Self::Parse(nom::Err::Failure(e)) => {
                Self::Parse(nom::Err::Failure(E::append(i, ek, e)))
            }
            _ => unreachable!(),
        }
    }
}

impl<I, E> nom::error::ContextError<I> for Error<E>
where
    E: nom::error::ContextError<I>,
{
    fn add_context(i: I, ctx: &'static str, other: Self) -> Self {
        match other {
            Self::Parse(nom::Err::Error(e)) => {
                Self::Parse(nom::Err::Error(E::add_context(i, ctx, e)))
            }
            Self::Parse(nom::Err::Failure(e)) => {
                Self::Parse(nom::Err::Failure(E::add_context(i, ctx, e)))
            }
            _ => unreachable!(),
        }
    }
}

impl<I> Error<nom::error::Error<I>> {
    pub fn map_input<T, F>(self, f: F) -> Error<nom::error::Error<T>>
    where
        F: FnOnce(I) -> T,
    {
        match self {
            Self::Parse(ep) => {
                let x = ep.map_input(f);
                Error::Parse(x)
            }
            Self::IO(e) => Error::IO(e),
            Self::TLS(e) => Error::TLS(e),
        }
    }
}

impl<I> Error<nom::error::VerboseError<I>> {
    pub fn map_input<T, F>(self, f: F) -> Error<nom::error::VerboseError<T>>
    where
        F: Fn(I) -> T,
    {
        match self {
            Self::Parse(ep) => Error::Parse(match ep {
                nom::Err::Error(mut err) => {
                    let errors = err.errors.drain(..).map(|(i, ek)| (f(i), ek)).collect();
                    nom::Err::Error(nom::error::VerboseError { errors })
                }
                nom::Err::Failure(mut err) => {
                    let errors = err.errors.drain(..).map(|(i, ek)| (f(i), ek)).collect();
                    nom::Err::Failure(nom::error::VerboseError { errors })
                }
                nom::Err::Incomplete(i) => nom::Err::Incomplete(i),
            }),
            Self::IO(e) => Error::IO(e),
            Self::TLS(e) => Error::TLS(e),
        }
    }
}

impl<E> std::cmp::PartialEq for Error<E>
where
    E: std::cmp::PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Parse(l0), Self::Parse(r0)) => l0 == r0,
            _ => false,
        }
    }
}
