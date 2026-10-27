//! Lossless native Engine performance codecs. Positions remain samples, not seconds.
mod slots;
pub use slots::{Cue, CueData, Loop, LoopData};

use crate::{Error, Result};
use flate2::{Compression, read::ZlibDecoder, write::ZlibEncoder};
use serde::{Deserialize, Serialize};
use std::io::{Read, Write};

/// Raw database representation. Preserve caches unchanged when editing unrelated metadata.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PerformanceData {
    pub track_id: i64,
    pub track_data: Option<Vec<u8>>,
    pub overview_wave_form_data: Option<Vec<u8>>,
    pub beat_data: Option<Vec<u8>>,
    pub quick_cues: Option<Vec<u8>>,
    pub loops: Option<Vec<u8>>,
    pub third_party_source_id: Option<i64>,
    pub active_on_load_loops: Option<i64>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct DecodedPerformance {
    pub beats: Option<BeatData>,
    pub cues: Option<CueData>,
    pub loops: Option<LoopData>,
}
impl PerformanceData {
    pub fn decode(&self, limit: usize) -> Result<DecodedPerformance> {
        Ok(DecodedPerformance {
            beats: self
                .beat_data
                .as_deref()
                .map(|bytes| BeatData::decode(&q_uncompress(bytes, limit)?))
                .transpose()?,
            cues: self
                .quick_cues
                .as_deref()
                .map(|bytes| CueData::decode(&q_uncompress(bytes, limit)?))
                .transpose()?,
            loops: self.loops.as_deref().map(LoopData::decode).transpose()?,
        })
    }
}

pub fn q_compress(bytes: &[u8]) -> Result<Vec<u8>> {
    let length =
        u32::try_from(bytes.len()).map_err(|_| invalid("compression exceeds u32 length"))?;
    let mut encoder = ZlibEncoder::new(length.to_be_bytes().to_vec(), Compression::default());
    encoder.write_all(bytes)?;
    Ok(encoder.finish()?)
}

/// Decode Qt's length-prefixed zlib format with a caller-owned allocation bound.
pub fn q_uncompress(bytes: &[u8], limit: usize) -> Result<Vec<u8>> {
    let mut input = Input(bytes);
    let expected = u32::from_be_bytes(input.array()?) as usize;
    if expected > limit {
        return Err(invalid("declared decompressed size exceeds limit"));
    }
    let mut decoder = ZlibDecoder::new(input.0);
    let mut output = Vec::new();
    decoder
        .by_ref()
        .take(expected as u64 + 1)
        .read_to_end(&mut output)?;
    if output.len() != expected {
        return Err(invalid("decompressed length differs from header"));
    }
    // Read to EOF to validate the checksum, without accepting excess output.
    let mut extra = [0];
    if decoder.read(&mut extra)? != 0 {
        return Err(invalid("decompressed data exceeds header"));
    }
    if decoder.total_in() != input.0.len() as u64 {
        return Err(invalid("trailing compressed bytes"));
    }
    Ok(output)
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BeatMarker {
    pub samples: f64,
    pub beat: i64,
    pub beats_to_next: u32,
    pub unknown: u32,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct BeatData {
    pub sample_rate: f64,
    pub track_samples: f64,
    pub is_set: u8,
    pub original: Vec<BeatMarker>,
    pub adjusted: Vec<BeatMarker>,
    pub trailing: Vec<u8>,
}

impl BeatData {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut input = Input(bytes);
        let sample_rate = f64::from_be_bytes(input.array()?);
        sample_rate_valid(sample_rate)?;
        let track_samples = f64::from_be_bytes(input.array()?);
        finite(track_samples)?;
        let is_set = input.byte()?;
        let original = read_grid(&mut input)?;
        let adjusted = read_grid(&mut input)?;
        Ok(Self {
            sample_rate,
            track_samples,
            is_set,
            original,
            adjusted,
            trailing: input.0.to_vec(),
        })
    }

    pub fn encode(&self) -> Result<Vec<u8>> {
        sample_rate_valid(self.sample_rate)?;
        finite(self.track_samples)?;
        let mut output = self.sample_rate.to_be_bytes().to_vec();
        output.extend(self.track_samples.to_be_bytes());
        output.push(self.is_set);
        for grid in [&self.original, &self.adjusted] {
            output.extend((grid.len() as u64).to_be_bytes());
            for marker in grid {
                finite(marker.samples)?;
                output.extend(marker.samples.to_le_bytes());
                output.extend(marker.beat.to_le_bytes());
                output.extend(marker.beats_to_next.to_le_bytes());
                output.extend(marker.unknown.to_le_bytes());
            }
        }
        output.extend(&self.trailing);
        Ok(output)
    }
}

fn read_grid(input: &mut Input<'_>) -> Result<Vec<BeatMarker>> {
    let count = u64::from_be_bytes(input.array()?);
    if count > (input.0.len() / 24) as u64 {
        return Err(invalid("truncated beat markers"));
    }
    let mut result = Vec::with_capacity(count as usize);
    for _ in 0..count {
        let samples = f64::from_le_bytes(input.array()?);
        finite(samples)?;
        result.push(BeatMarker {
            samples,
            beat: i64::from_le_bytes(input.array()?),
            beats_to_next: u32::from_le_bytes(input.array()?),
            unknown: u32::from_le_bytes(input.array()?),
        });
    }
    Ok(result)
}

fn invalid(message: &str) -> Error {
    Error::Codec(message.into())
}
fn finite(value: f64) -> Result<()> {
    if !value.is_finite() {
        return Err(invalid("nonfinite sample position"));
    }
    Ok(())
}
pub(crate) fn sample_rate_valid(value: f64) -> Result<()> {
    if !value.is_finite() || value <= 0.0 {
        return Err(invalid("sample rate must be positive and finite"));
    }
    Ok(())
}
struct Input<'a>(&'a [u8]);
impl<'a> Input<'a> {
    fn take(&mut self, size: usize) -> Result<&'a [u8]> {
        let (value, rest) = self
            .0
            .split_at_checked(size)
            .ok_or_else(|| invalid("truncated field"))?;
        self.0 = rest;
        Ok(value)
    }
    fn byte(&mut self) -> Result<u8> {
        Ok(self.take(1)?[0])
    }
    fn array<const N: usize>(&mut self) -> Result<[u8; N]> {
        Ok(self.take(N)?.try_into().expect("checked fixed width"))
    }
    fn name(&mut self) -> Result<String> {
        let size = self.byte()? as usize;
        String::from_utf8(self.take(size)?.to_vec()).map_err(|_| invalid("label is not UTF-8"))
    }
}
fn write_name(output: &mut Vec<u8>, name: &str) -> Result<()> {
    output.push(u8::try_from(name.len()).map_err(|_| invalid("UTF-8 label exceeds 255 bytes"))?);
    output.extend(name.as_bytes());
    Ok(())
}
