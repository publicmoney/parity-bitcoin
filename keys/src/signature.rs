//! Bitcoin signatures.
//!
//! http://bitcoin.stackexchange.com/q/12554/40688

use crate::Error;
use bitcrypto::{FromHex, ToHex};
use std::{fmt, ops, str};

#[derive(PartialEq)]
pub struct Signature(Vec<u8>);

impl fmt::Debug for Signature {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.0.to_hex().fmt(f)
	}
}

impl fmt::Display for Signature {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		self.0.to_hex().fmt(f)
	}
}

impl ops::Deref for Signature {
	type Target = [u8];

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl str::FromStr for Signature {
	type Err = Error;

	fn from_str(s: &str) -> Result<Self, Error> {
		let vec = FromHex::from_hex(s).map_err(|_| Error::InvalidSignature)?;
		Ok(Signature(vec))
	}
}

impl From<&'static str> for Signature {
	fn from(s: &'static str) -> Self {
		s.parse().unwrap()
	}
}

impl From<Vec<u8>> for Signature {
	fn from(v: Vec<u8>) -> Self {
		Signature(v)
	}
}

impl From<Signature> for Vec<u8> {
	fn from(s: Signature) -> Self {
		s.0
	}
}

impl Signature {
	pub fn check_low_s(&self) -> bool {
		unimplemented!();
	}
}

impl<'a> From<&'a [u8]> for Signature {
	fn from(v: &'a [u8]) -> Self {
		Signature(v.to_vec())
	}
}

pub struct CompactSignature(pub [u8; 65]);

impl fmt::Debug for CompactSignature {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.write_str(&self.0.to_hex())
	}
}

impl fmt::Display for CompactSignature {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		f.write_str(&self.0.to_hex())
	}
}

impl ops::Deref for CompactSignature {
	type Target = [u8];

	fn deref(&self) -> &Self::Target {
		&self.0
	}
}

impl PartialEq for CompactSignature {
	fn eq(&self, other: &Self) -> bool {
		let s_slice: &[u8] = self;
		let o_slice: &[u8] = other;
		s_slice == o_slice
	}
}

impl str::FromStr for CompactSignature {
	type Err = Error;

	fn from_str(s: &str) -> Result<Self, Error> {
		let sig = FromHex::from_hex(s).map_err(|_| Error::InvalidSignature)?;
		Ok(CompactSignature(sig))
	}
}

impl From<&'static str> for CompactSignature {
	fn from(s: &'static str) -> Self {
		s.parse().unwrap()
	}
}
