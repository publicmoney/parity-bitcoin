//
// Copyright 2018-2019 Tamas Blummer
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//
//!
//! # The data file
//! Specific implementation details to data file
//!

use crate::error::Error;
use crate::format::{Data, Envelope, IndexedData, Link, Payload};
use crate::page::{PAGE_PAYLOAD_SIZE, PAGE_SIZE};
use crate::pagedfile::{PagedFile, PagedFileAppender};
use crate::pref::PRef;

use byteorder::{BigEndian, ByteOrder};

/// file storing indexed and referred data
pub struct DataFile {
	appender: PagedFileAppender,
}

impl DataFile {
	/// create new file
	pub fn new(file: Box<dyn PagedFile>) -> Result<DataFile, Error> {
		let len = file.len()?;
		if len % PAGE_SIZE as u64 != 0 {
			return Err(Error::Corrupted("data file does not end at page boundary".to_string()));
		}
		if len >= PAGE_SIZE as u64 {
			return Ok(DataFile {
				appender: PagedFileAppender::new(file, PRef::from(len)),
			});
		} else {
			let appender = PagedFileAppender::new(file, PRef::from(0));
			return Ok(DataFile { appender });
		}
	}

	/// return an iterator of all payloads
	pub fn envelopes<'a>(&'a self) -> EnvelopeIterator<'a> {
		EnvelopeIterator::new(&self.appender)
	}

	/// shutdown
	pub fn shutdown(&mut self) {
		self.appender.shutdown()
	}

	/// get a stored content at pref
	pub fn get_envelope(&self, mut pref: PRef) -> Result<Envelope, Error> {
		let mut len = [0u8; 3];
		pref = self.appender.read(pref, &mut len, 3)?;
		let blen = BigEndian::read_u24(&len) as usize;
		if blen >= PAGE_PAYLOAD_SIZE {
			let mut buf = vec![0u8; blen];
			self.appender.read(pref, &mut buf, blen)?;
			Ok(Envelope::deseralize(buf))
		} else {
			let mut buf = [0u8; PAGE_PAYLOAD_SIZE]; // TODO why read so much by default? rather than just the length?
			self.appender.read(pref, &mut buf, blen)?;
			Ok(Envelope::deseralize(buf[0..blen].to_vec()))
		}
	}

	/// append link
	pub fn append_link(&mut self, link: Link) -> Result<PRef, Error> {
		let mut payload = vec![];
		Payload::Link(link).serialize(&mut payload);
		let envelope = Envelope::new(payload.as_slice());
		let mut store = vec![];
		envelope.serialize(&mut store);
		let me = self.appender.position();
		self.appender.append(store.as_slice())?;
		Ok(me)
	}

	/// append indexed data
	pub fn append_data(&mut self, key: &[u8], data: &[u8]) -> Result<PRef, Error> {
		let indexed = IndexedData::new(key, Data::new(data));
		let mut payload = vec![];
		Payload::Indexed(indexed).serialize(&mut payload);
		let envelope = Envelope::new(payload.as_slice());
		let mut store = vec![];
		envelope.serialize(&mut store);
		let me = self.appender.position();
		self.appender.append(store.as_slice())?;
		Ok(me)
	}

	/// append referred data
	pub fn append_referred(&mut self, data: &[u8]) -> Result<PRef, Error> {
		let data = Data::new(data);
		let mut payload = vec![];
		Payload::Referred(data).serialize(&mut payload);
		let envelope = Envelope::new(payload.as_slice());
		let mut store = vec![];
		envelope.serialize(&mut store);
		let me = self.appender.position();
		self.appender.append(store.as_slice())?;
		Ok(me)
	}

	pub fn set_data(&mut self, pref: PRef, data: &[u8]) -> Result<PRef, Error> {
		let envelope = self.get_envelope(pref)?;

		let new_payload = match Payload::deserialize(envelope.payload())? {
			Payload::Indexed(mut p) => {
				p.data.data = data;
				Payload::Indexed(p)
			}
			Payload::Referred(mut p) => {
				p.data = data;
				Payload::Referred(p)
			}
			_ => panic!("Links should not be updated"),
		};

		let new_envelope = Envelope::from_payload(new_payload);

		if envelope.payload().len() != new_envelope.payload().len() {
			return Err(Error::ValueTooLong);
		}

		let mut store = vec![];
		new_envelope.serialize(&mut store);

		self.appender.update(pref, &store)?;
		Ok(pref)
	}

	/// truncate file
	pub fn truncate(&mut self, pref: u64) -> Result<(), Error> {
		self.appender.truncate(pref)
	}

	/// flush buffers
	pub fn flush(&mut self) -> Result<(), Error> {
		self.appender.flush()
	}

	/// sync file on file system
	pub fn sync(&self) -> Result<(), Error> {
		self.appender.sync()
	}

	/// get file length
	pub fn len(&self) -> Result<u64, Error> {
		self.appender.len()
	}
}

/// Iterate data file content
pub struct EnvelopeIterator<'f> {
	file: &'f PagedFileAppender,
	pos: PRef,
}

impl<'f> EnvelopeIterator<'f> {
	/// create a new iterator
	pub fn new(file: &'f PagedFileAppender) -> EnvelopeIterator<'f> {
		EnvelopeIterator { file, pos: PRef::from(0) }
	}
}

impl<'f> Iterator for EnvelopeIterator<'f> {
	type Item = (PRef, Envelope);

	fn next(&mut self) -> Option<<Self as Iterator>::Item> {
		if self.pos.is_valid() {
			let start = self.pos;
			let mut len = [0u8; 3];
			if let Ok(pos) = self.file.read(start, &mut len, 3) {
				let length = BigEndian::read_u24(&len) as usize;
				if length > 0 {
					let mut buf = vec![0u8; length];
					self.pos = self.file.read(pos, &mut buf, length).unwrap();
					let envelope = Envelope::deseralize(buf);
					return Some((start, envelope));
				}
			}
		}
		None
	}
}
