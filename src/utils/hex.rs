use std::fmt;
use std::ops::Deref;

pub enum Hex<'a> {
    Ref(&'a [u8]),
    Owned(Vec<u8>),
}

impl<'a> Hex<'a> {
    pub fn as_slice(&'a self) -> &'a [u8] {
        self.as_ref()
    }
}

impl<'a> From<&'a [u8]> for Hex<'a> {
    fn from(h: &'a [u8]) -> Self {
        Self::Ref(h)
    }
}

impl From<Vec<u8>> for Hex<'_> {
    fn from(v: Vec<u8>) -> Self {
        Self::Owned(v)
    }
}

impl Deref for Hex<'_> {
    type Target = [u8];

    fn deref(&self) -> &Self::Target {
        match self {
            Self::Ref(h) => h,
            Self::Owned(o) => &o[..],
        }
    }
}

impl AsRef<[u8]> for Hex<'_> {
    fn as_ref(&self) -> &[u8] {
        match self {
            Self::Ref(r) => r,
            Self::Owned(o) => &o[..],
        }
    }
}

impl fmt::Debug for Hex<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut buffer = self.as_ref();
        f.write_str("\"")?;
        loop {
            match std::str::from_utf8(buffer) {
                Ok(s) => {
                    write!(f, "{}", s.escape_debug())?;
                    break;
                }
                Err(e) => {
                    let valid = &buffer[..e.valid_up_to()];
                    f.write_str(unsafe { std::str::from_utf8_unchecked(valid) })?;
                    write!(f, "\\x{:02x}", buffer[0])?;
                    buffer = &buffer[1..];
                }
            }
        }

        f.write_str("\"")?;
        Ok(())
    }
}

impl std::cmp::PartialEq for Hex<'_> {
    fn eq(&self, other: &Self) -> bool {
        std::cmp::PartialEq::eq(self.as_slice(), other.as_slice())
    }
}
