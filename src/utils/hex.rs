use std::{borrow::Cow, fmt, ops::Deref};

pub struct Hex<'a>(Cow<'a, [u8]>);

impl<'a> Hex<'a> {
    pub fn as_slice(&'a self) -> &'a [u8] {
        self.as_ref()
    }
}

impl<'a> From<&'a [u8]> for Hex<'a> {
    fn from(v: &'a [u8]) -> Self {
        Self(v.into())
    }
}

impl From<Vec<u8>> for Hex<'_> {
    fn from(v: Vec<u8>) -> Self {
        Self(v.into())
    }
}

impl<'a> From<Cow<'a, [u8]>> for Hex<'a> {
    fn from(v: Cow<'a, [u8]>) -> Self {
        Self(v)
    }
}

impl Deref for Hex<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        self.0.deref()
    }
}

impl AsRef<[u8]> for Hex<'_> {
    fn as_ref(&self) -> &[u8] {
        self.0.as_ref()
    }
}

impl fmt::Debug for Hex<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let buffer = self.as_ref();
        for b in buffer {
            match *b {
                b'\n' => f.write_str("\\n\n")?,
                b'\r' => f.write_str("\\r")?,
                b'\t' => f.write_str("\\t")?,
                _ => {
                    if b.is_ascii_control() {
                        write!(f, "\\x{b:02x}")?
                    } else {
                        let s = &[*b][..];
                        let s = unsafe { std::str::from_utf8_unchecked(s) };
                        f.write_str(s)?
                    }
                }
            }
        }

        Ok(())
    }
}

impl std::cmp::PartialEq for Hex<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::cmp::PartialEq::eq(self.as_slice(), other.as_slice())
    }
}
