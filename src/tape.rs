//! Streaming protocol-event tapes. Payloads are decoded by the pinned host adapter.
//!
//! No network access or Minecraft dependency lives here. Recovery accepts only an
//! incomplete final frame after a valid metadata frame; complete corrupt frames fail.
use serde::{Deserialize, Serialize};
use std::io::{self, Read, Write};

const MAGIC: &[u8; 8] = b"HFNET001";
const FRAME_HEADER: usize = 21;
pub const MAX_PAYLOAD: usize = 16 * 1024 * 1024;
pub const MAX_TAPE_BYTES: u64 = 1024 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TapeMeta {
    pub schema: u32,
    pub horizon_revision: String,
    pub protocol: i32,
    pub game_version: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PacketRecord {
    /// Game ticks since capture began, at 20 ticks per second.
    pub tick: u64,
    /// Adapter-defined event kind. Zero is reserved for metadata.
    pub kind: u8,
    pub payload: Vec<u8>,
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message)
}
fn metadata_valid(meta: &TapeMeta) -> bool {
    meta.schema == 1
        && meta.protocol > 0
        && meta.horizon_revision.len() == 40
        && meta.horizon_revision.bytes().all(|b| b.is_ascii_hexdigit())
        && !meta.game_version.is_empty()
        && meta.game_version.len() <= 64
}
fn header_crc(length: u32, tick: u64, kind: u8) -> u32 {
    let mut hash = crc32fast::Hasher::new();
    hash.update(&length.to_le_bytes());
    hash.update(&tick.to_le_bytes());
    hash.update(&[kind]);
    hash.finalize()
}
fn frame(writer: &mut impl Write, tick: u64, kind: u8, payload: &[u8]) -> io::Result<()> {
    if payload.len() > MAX_PAYLOAD {
        return Err(invalid("network tape frame exceeds 16 MiB"));
    }
    writer.write_all(&(payload.len() as u32).to_le_bytes())?;
    writer.write_all(&header_crc(payload.len() as u32, tick, kind).to_le_bytes())?;
    writer.write_all(&crc32fast::hash(payload).to_le_bytes())?;
    writer.write_all(&tick.to_le_bytes())?;
    writer.write_all(&[kind])?;
    writer.write_all(payload)
}

pub struct TapeWriter<W: Write> {
    writer: W,
    bytes: u64,
    last_tick: u64,
    failed: bool,
}
impl<W: Write> TapeWriter<W> {
    pub fn new(mut writer: W, meta: &TapeMeta) -> io::Result<Self> {
        if !metadata_valid(meta) {
            return Err(invalid("invalid network tape metadata"));
        }
        let metadata = serde_json::to_vec(meta).map_err(io::Error::other)?;
        writer.write_all(MAGIC)?;
        frame(&mut writer, 0, 0, &metadata)?;
        writer.flush()?;
        Ok(Self {
            writer,
            bytes: (MAGIC.len() + FRAME_HEADER + metadata.len()) as u64,
            last_tick: 0,
            failed: false,
        })
    }
    pub fn append(&mut self, record: &PacketRecord) -> io::Result<()> {
        if self.failed {
            return Err(invalid("network tape writer failed; start a new tape"));
        }
        if record.kind == 0 || record.tick < self.last_tick {
            return Err(invalid("invalid network tape event order or kind"));
        }
        let total = self
            .bytes
            .checked_add(FRAME_HEADER as u64 + record.payload.len() as u64)
            .ok_or_else(|| invalid("network tape length overflow"))?;
        if total > MAX_TAPE_BYTES {
            return Err(invalid("network tape exceeds 1 GiB"));
        }
        if let Err(error) = frame(&mut self.writer, record.tick, record.kind, &record.payload) {
            self.failed = true;
            return Err(error);
        }
        self.bytes = total;
        self.last_tick = record.tick;
        Ok(())
    }
    pub fn flush(&mut self) -> io::Result<()> {
        if self.failed {
            return Err(invalid("network tape writer failed"));
        }
        if let Err(error) = self.writer.flush() {
            self.failed = true;
            return Err(error);
        }
        Ok(())
    }
    pub fn into_inner(mut self) -> io::Result<W> {
        self.flush()?;
        Ok(self.writer)
    }
    pub fn bytes_written(&self) -> u64 {
        self.bytes
    }
}

pub struct TapeReader<R: Read> {
    reader: R,
    pub metadata: TapeMeta,
    pub recovered_tail: bool,
    bytes: u64,
    last_tick: u64,
    ended: bool,
}

enum FrameRead {
    Eof,
    Incomplete,
    Record(PacketRecord),
}
fn read_frame(reader: &mut impl Read, bytes: &mut u64) -> io::Result<FrameRead> {
    let mut header = [0u8; FRAME_HEADER];
    let mut read = 0;
    while read < header.len() {
        match reader.read(&mut header[read..]) {
            Ok(0) => {
                return Ok(if read == 0 {
                    FrameRead::Eof
                } else {
                    FrameRead::Incomplete
                });
            }
            Ok(count) => read += count,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    let length = u32::from_le_bytes(
        header[0..4]
            .try_into()
            .map_err(|_| invalid("bad frame header"))?,
    ) as usize;
    if length > MAX_PAYLOAD {
        return Err(invalid("network tape frame exceeds 16 MiB"));
    }
    let expected_header = u32::from_le_bytes(
        header[4..8]
            .try_into()
            .map_err(|_| invalid("bad frame header"))?,
    );
    let expected_payload = u32::from_le_bytes(
        header[8..12]
            .try_into()
            .map_err(|_| invalid("bad frame header"))?,
    );
    let tick = u64::from_le_bytes(
        header[12..20]
            .try_into()
            .map_err(|_| invalid("bad frame header"))?,
    );
    let kind = header[20];
    if header_crc(length as u32, tick, kind) != expected_header {
        return Err(invalid("network tape frame header checksum mismatch"));
    }
    *bytes = bytes
        .checked_add(FRAME_HEADER as u64 + length as u64)
        .ok_or_else(|| invalid("network tape length overflow"))?;
    if *bytes > MAX_TAPE_BYTES {
        return Err(invalid("network tape exceeds 1 GiB"));
    }
    let mut payload = vec![0; length];
    let mut read = 0;
    while read < length {
        match reader.read(&mut payload[read..]) {
            Ok(0) => return Ok(FrameRead::Incomplete),
            Ok(count) => read += count,
            Err(e) if e.kind() == io::ErrorKind::Interrupted => continue,
            Err(e) => return Err(e),
        }
    }
    if crc32fast::hash(&payload) != expected_payload {
        return Err(invalid("network tape payload checksum mismatch"));
    }
    Ok(FrameRead::Record(PacketRecord {
        tick,
        kind,
        payload,
    }))
}
impl<R: Read> TapeReader<R> {
    pub fn new(mut reader: R) -> io::Result<Self> {
        let mut magic = [0; 8];
        reader.read_exact(&mut magic)?;
        if &magic != MAGIC {
            return Err(invalid("not a Horizon network tape"));
        }
        let mut bytes = 8;
        let FrameRead::Record(header) = read_frame(&mut reader, &mut bytes)? else {
            return Err(invalid("incomplete network tape metadata"));
        };
        if header.kind != 0 || header.tick != 0 {
            return Err(invalid("network tape metadata must be first"));
        }
        let metadata: TapeMeta =
            serde_json::from_slice(&header.payload).map_err(io::Error::other)?;
        if !metadata_valid(&metadata) {
            return Err(invalid("unsupported network tape metadata"));
        }
        Ok(Self {
            reader,
            metadata,
            recovered_tail: false,
            bytes,
            last_tick: 0,
            ended: false,
        })
    }
    pub fn next_record(&mut self) -> io::Result<Option<PacketRecord>> {
        if self.ended {
            return Ok(None);
        }
        let result = (|| match read_frame(&mut self.reader, &mut self.bytes)? {
            FrameRead::Record(record) => {
                if record.kind == 0 || record.tick < self.last_tick {
                    return Err(invalid("network tape event order or kind is invalid"));
                }
                self.last_tick = record.tick;
                Ok(Some(record))
            }
            FrameRead::Incomplete => {
                self.recovered_tail = true;
                self.ended = true;
                Ok(None)
            }
            FrameRead::Eof => {
                self.ended = true;
                Ok(None)
            }
        })();
        if result.is_err() {
            self.ended = true;
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn metadata() -> TapeMeta {
        TapeMeta {
            schema: 1,
            horizon_revision: "a".repeat(40),
            protocol: 1,
            game_version: "26.3".into(),
        }
    }
    fn tape() -> (Vec<u8>, usize) {
        let mut writer = TapeWriter::new(Vec::new(), &metadata()).unwrap();
        writer
            .append(&PacketRecord {
                tick: 0,
                kind: 1,
                payload: vec![1, 2],
            })
            .unwrap();
        let boundary = writer.bytes_written() as usize;
        writer
            .append(&PacketRecord {
                tick: 10,
                kind: 2,
                payload: vec![3, 4, 5],
            })
            .unwrap();
        (writer.into_inner().unwrap(), boundary)
    }
    #[test]
    fn stream_roundtrip_and_incomplete_tail_recovery() {
        let (bytes, boundary) = tape();
        let mut reader = TapeReader::new(bytes.as_slice()).unwrap();
        assert_eq!(reader.next_record().unwrap().unwrap().payload, [1, 2]);
        assert_eq!(reader.next_record().unwrap().unwrap().tick, 10);
        assert!(reader.next_record().unwrap().is_none());
        assert!(!reader.recovered_tail);
        for end in boundary + 1..bytes.len() {
            let mut reader = TapeReader::new(&bytes[..end]).unwrap();
            reader.next_record().unwrap();
            assert!(reader.next_record().unwrap().is_none());
            assert!(reader.recovered_tail);
        }
    }
    #[test]
    fn complete_corruption_is_never_treated_as_recovery() {
        let (bytes, boundary) = tape();
        for index in boundary..bytes.len() {
            let mut bad = bytes.clone();
            bad[index] ^= 1;
            let mut reader = TapeReader::new(bad.as_slice()).unwrap();
            reader.next_record().unwrap();
            assert!(
                reader.next_record().is_err(),
                "corrupt byte {index} accepted"
            );
        }
    }
    #[test]
    fn metadata_cannot_be_truncated_and_order_is_checked() {
        let writer = TapeWriter::new(Vec::new(), &metadata()).unwrap();
        let bytes = writer.into_inner().unwrap();
        for end in 0..bytes.len() {
            assert!(TapeReader::new(&bytes[..end]).is_err());
        }
        let mut writer = TapeWriter::new(Vec::new(), &metadata()).unwrap();
        writer
            .append(&PacketRecord {
                tick: 5,
                kind: 1,
                payload: vec![],
            })
            .unwrap();
        assert!(
            writer
                .append(&PacketRecord {
                    tick: 4,
                    kind: 1,
                    payload: vec![]
                })
                .is_err()
        );
        assert!(
            writer
                .append(&PacketRecord {
                    tick: 5,
                    kind: 0,
                    payload: vec![]
                })
                .is_err()
        );
    }
}
