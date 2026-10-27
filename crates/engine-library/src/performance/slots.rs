use super::{Input, Result, finite, invalid, write_name};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Cue {
    pub name: String,
    pub samples: f64,
    pub color: [u8; 4],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CueData {
    pub slots: [Option<Cue>; 8],
    pub modified: f64,
    pub overridden: u8,
    pub original: f64,
    pub trailing: Vec<u8>,
}

impl Default for CueData {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
            modified: -1.0,
            overridden: 0,
            original: -1.0,
            trailing: Vec::new(),
        }
    }
}

impl CueData {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut input = Input(bytes);
        if u64::from_be_bytes(input.array()?) != 8 {
            return Err(invalid("cue count must be 8"));
        }
        let mut result = Self::default();
        for slot in &mut result.slots {
            let name = input.name()?;
            let samples = f64::from_be_bytes(input.array()?);
            finite(samples)?;
            let color = input.array()?;
            if samples >= 0.0 {
                *slot = Some(Cue {
                    name,
                    samples,
                    color,
                });
            }
        }
        result.modified = f64::from_be_bytes(input.array()?);
        finite(result.modified)?;
        result.overridden = input.byte()?;
        result.original = f64::from_be_bytes(input.array()?);
        finite(result.original)?;
        result.trailing = input.0.to_vec();
        Ok(result)
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut output = 8_u64.to_be_bytes().to_vec();
        for slot in &self.slots {
            if let Some(cue) = slot {
                finite(cue.samples)?;
                if cue.samples < 0.0 {
                    return Err(invalid("negative hot cue"));
                }
                write_name(&mut output, &cue.name)?;
                output.extend(cue.samples.to_be_bytes());
                output.extend(cue.color);
            } else {
                output.push(0);
                output.extend((-1_f64).to_be_bytes());
                output.extend([0; 4]);
            }
        }
        finite(self.modified)?;
        finite(self.original)?;
        output.extend(self.modified.to_be_bytes());
        output.push(self.overridden);
        output.extend(self.original.to_be_bytes());
        output.extend(&self.trailing);
        Ok(output)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Loop {
    pub name: String,
    pub start: f64,
    pub end: f64,
    pub flags: [u8; 3],
    pub color: [u8; 3],
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LoopData {
    pub slots: [Option<Loop>; 8],
    pub trailing: Vec<u8>,
}
impl Default for LoopData {
    fn default() -> Self {
        Self {
            slots: std::array::from_fn(|_| None),
            trailing: Vec::new(),
        }
    }
}

impl LoopData {
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let mut input = Input(bytes);
        if input.take(8)? != [8, 0, 0, 0, 0, 0, 0, 0] {
            return Err(invalid("invalid loop header"));
        }
        let mut result = Self::default();
        for slot in &mut result.slots {
            let name = input.name()?;
            let start = f64::from_le_bytes(input.array()?);
            let end = f64::from_le_bytes(input.array()?);
            finite(start)?;
            finite(end)?;
            let flags = input.array()?;
            let color = input.array()?;
            if start >= 0.0 && end >= start {
                *slot = Some(Loop {
                    name,
                    start,
                    end,
                    flags,
                    color,
                });
            }
        }
        result.trailing = input.0.to_vec();
        Ok(result)
    }
    pub fn encode(&self) -> Result<Vec<u8>> {
        let mut output = vec![8, 0, 0, 0, 0, 0, 0, 0];
        for slot in &self.slots {
            if let Some(value) = slot {
                finite(value.start)?;
                finite(value.end)?;
                if value.start < 0.0 || value.end < value.start {
                    return Err(invalid("invalid loop bounds"));
                }
                write_name(&mut output, &value.name)?;
                output.extend(value.start.to_le_bytes());
                output.extend(value.end.to_le_bytes());
                output.extend(value.flags);
                output.extend(value.color);
            } else {
                output.push(0);
                output.extend((-1_f64).to_le_bytes());
                output.extend((-1_f64).to_le_bytes());
                output.extend([0; 6]);
            }
        }
        output.extend(&self.trailing);
        Ok(output)
    }
}
