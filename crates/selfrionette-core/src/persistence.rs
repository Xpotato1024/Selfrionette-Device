//! 未検証の合成profileだけを保存する有限store。再読込でbaseline/readinessを復元しない。
use crate::DeviceId;
use crate::calibration::{CalibrationSession, Context, Draft, N, Observation, Source};
use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
pub const MAX_RECORD_BYTES: usize = 32768;
pub const MAX_RECORDS_PER_ID: usize = 32;
fn hex(s: &str) -> Result<String, String> {
    if s.trim().is_empty() || s.len() > 1024 {
        return Err("empty/oversize metadata".into());
    }
    Ok(s.as_bytes().iter().map(|b| format!("{b:02x}")).collect())
}
fn unhex(s: &str) -> Result<String, String> {
    if !s.is_ascii() || s.is_empty() || s.len() > 2048 || s.len() % 2 != 0 {
        return Err("invalid metadata encoding".into());
    }
    let bytes = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).map_err(|_| "invalid hex".to_string()))
        .collect::<Result<Vec<_>, _>>()?;
    String::from_utf8(bytes).map_err(|_| "invalid UTF-8 metadata".into())
}
pub fn encode(d: &Draft) -> Result<String, String> {
    if d.revision == 0 || d.context.source.epoch == 0 {
        return Err("invalid revision/epoch".into());
    }
    if !d.context.source.firmware.starts_with("synthetic-")
        || !d.context.source.assembly.starts_with("fixture-")
    {
        return Err("only unverified synthetic records supported".into());
    }
    let mut text = format!(
        "srn-preview,1\nhardware_verified,0\ncontrol_ready,0\nsource,{},{},{}\noperation,{},{},{},{}\n",
        d.context.source.id,
        hex(&d.context.source.firmware)?,
        hex(&d.context.source.assembly)?,
        hex(&d.context.user)?,
        hex(&d.context.posture)?,
        d.context.source.epoch,
        d.revision
    );
    for i in 0..N {
        if ![-1, 1].contains(&d.signs[i])
            || !d.positive_spans[i].is_finite()
            || d.positive_spans[i] <= 0.0
            || d.reverse_spans[i].is_some_and(|v| !v.is_finite() || v <= 0.0)
        {
            return Err("invalid sign/span".into());
        }
        text.push_str(&format!(
            "channel,{i},{},{},{},{},{}\n",
            d.signs[i],
            d.positive_spans[i],
            d.reverse_spans[i]
                .map(|v| v.to_string())
                .unwrap_or_else(|| "?".into()),
            u8::from(d.manual[i]),
            hex(&d.evidence[i])?
        ));
    }
    let o = &d.observation;
    if o.firmware_sample
        .iter()
        .chain(o.baseline.iter())
        .chain(o.delta.iter())
        .chain(o.oriented.iter())
        .any(|v| !v.is_finite())
        || o.normalized.iter().flatten().any(|v| !v.is_finite())
    {
        return Err("invalid observation".into());
    }
    if o.timestamp_ms > u64::from(u32::MAX) {
        return Err("timestamp outside firmware u32 domain".into());
    }
    for ch in 0..N {
        let expected_delta = o.firmware_sample[ch] - o.baseline[ch];
        let expected_oriented = f64::from(d.signs[ch]) * expected_delta;
        let expected_normalized = if expected_oriented >= 0.0 {
            Some(expected_oriented / d.positive_spans[ch])
        } else {
            d.reverse_spans[ch].map(|span| expected_oriented / span)
        };
        if o.delta[ch] != expected_delta
            || o.oriented[ch] != expected_oriented
            || o.normalized[ch] != expected_normalized
            || o.normalized[ch].is_some_and(|n| n.abs() > 1.0)
        {
            return Err("inconsistent/unconfirmed observation".into());
        }
    }
    text.push_str(&format!("timestamp_ms,{}\n", o.timestamp_ms));
    for (key, values) in [
        ("firmware_sample", &o.firmware_sample),
        ("historical_baseline", &o.baseline),
        ("delta", &o.delta),
        ("oriented", &o.oriented),
    ] {
        text.push_str(&format!(
            "{key},{}\n",
            values.map(|v| v.to_string()).join(",")
        ));
    }
    text.push_str(&format!(
        "normalized,{}\n",
        o.normalized
            .map(|v| v.map(|n| n.to_string()).unwrap_or_else(|| "?".into()))
            .join(",")
    ));
    if text.len() > MAX_RECORD_BYTES {
        return Err("record size limit".into());
    }
    Ok(text)
}
pub fn decode(text: &str) -> Result<Draft, String> {
    if text.len() > MAX_RECORD_BYTES {
        return Err("record size limit".into());
    }
    let lines: Vec<_> = text.lines().collect();
    if lines.len() != 11 + N
        || lines[0] != "srn-preview,1"
        || lines[1] != "hardware_verified,0"
        || lines[2] != "control_ready,0"
    {
        return Err("schema/provenance/verification rejected".into());
    }
    let source: Vec<_> = lines[3].split(',').collect();
    let operation: Vec<_> = lines[4].split(',').collect();
    if source.len() != 4
        || source[0] != "source"
        || operation.len() != 5
        || operation[0] != "operation"
    {
        return Err("missing source/context".into());
    }
    let id = source[1].parse::<DeviceId>().map_err(|e| e.to_string())?;
    let context = Context {
        source: Source {
            id,
            firmware: unhex(source[2])?,
            assembly: unhex(source[3])?,
            epoch: operation[3].parse().map_err(|_| "invalid epoch")?,
        },
        user: unhex(operation[1])?,
        posture: unhex(operation[2])?,
    };
    let mut d = Draft {
        context,
        signs: [0; N],
        positive_spans: [0.0; N],
        reverse_spans: [None; N],
        evidence: std::array::from_fn(|_| String::new()),
        manual: [false; N],
        revision: operation[4].parse().map_err(|_| "invalid revision")?,
        observation: decode_observation(&lines[5 + N..])?,
    };
    for i in 0..N {
        let fields: Vec<_> = lines[5 + i].split(',').collect();
        if fields.len() != 7 || fields[0] != "channel" || fields[1] != i.to_string() {
            return Err("channel order/count rejected".into());
        }
        d.signs[i] = fields[2].parse().map_err(|_| "invalid sign")?;
        d.positive_spans[i] = fields[3].parse().map_err(|_| "invalid span")?;
        d.reverse_spans[i] = if fields[4] == "?" {
            None
        } else {
            Some(fields[4].parse().map_err(|_| "invalid reverse span")?)
        };
        d.manual[i] = match fields[5] {
            "0" => false,
            "1" => true,
            _ => return Err("invalid override origin".into()),
        };
        d.evidence[i] = unhex(fields[6])?;
    }
    // Encode validates all finite values, signs, bounded metadata and revisions.
    encode(&d)?;
    Ok(d)
}
fn decode_observation(lines: &[&str]) -> Result<Observation, String> {
    let timestamp = lines[0]
        .strip_prefix("timestamp_ms,")
        .ok_or("observation timestamp missing")?
        .parse()
        .map_err(|_| "invalid timestamp")?;
    let mut arrays = [[0.0; N]; 4];
    for (i, key) in [
        "firmware_sample",
        "historical_baseline",
        "delta",
        "oriented",
    ]
    .iter()
    .enumerate()
    {
        let fields: Vec<_> = lines[i + 1].split(',').collect();
        if fields.len() != N + 1 || fields[0] != *key {
            return Err("observation channel count/order".into());
        }
        for ch in 0..N {
            arrays[i][ch] = fields[ch + 1]
                .parse()
                .map_err(|_| "invalid observation number")?;
        }
    }
    let fields: Vec<_> = lines[5].split(',').collect();
    if fields.len() != N + 1 || fields[0] != "normalized" {
        return Err("normalized channel count/order".into());
    }
    let mut normalized = [None; N];
    for ch in 0..N {
        if fields[ch + 1] != "?" {
            normalized[ch] = Some(
                fields[ch + 1]
                    .parse()
                    .map_err(|_| "invalid normalized value")?,
            );
        }
    }
    Ok(Observation {
        timestamp_ms: timestamp,
        firmware_sample: arrays[0],
        baseline: arrays[1],
        delta: arrays[2],
        oriented: arrays[3],
        normalized,
    })
}
pub fn resume(d: &Draft, fresh: Context) -> Result<CalibrationSession, String> {
    if d.context.source.id != fresh.source.id
        || d.context.source.firmware != fresh.source.firmware
        || d.context.source.assembly != fresh.source.assembly
        || d.context.user != fresh.user
        || d.context.posture != fresh.posture
    {
        return Err("profile/source/user/posture mismatch".into());
    }
    d.reload(fresh).map_err(str::to_owned)
}
pub struct ProfileStore {
    root: PathBuf,
}
impl ProfileStore {
    pub fn new(root: PathBuf) -> Result<Self, String> {
        if !root.is_absolute() {
            return Err("absolute private store path required".into());
        }
        Ok(Self { root })
    }
    fn directory(&self, id: DeviceId, create: bool) -> Result<PathBuf, String> {
        if create {
            fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        }
        reject_link(&self.root)?;
        let directory = self.root.join(id.to_string());
        if create {
            if directory.exists() {
                reject_link(&directory)?;
            }
            fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        }
        reject_link(&directory)?;
        Ok(directory)
    }
    fn files(directory: &Path) -> Result<Vec<PathBuf>, String> {
        let mut files = vec![];
        for (count, entry) in fs::read_dir(directory)
            .map_err(|e| e.to_string())?
            .enumerate()
        {
            if count >= MAX_RECORDS_PER_ID {
                return Err("directory entry limit".into());
            }
            let path = entry.map_err(|e| e.to_string())?.path();
            if path.extension().is_some_and(|e| e == "srnp") {
                reject_link(&path)?;
                sequence(&path)?;
                files.push(path);
            }
            if files.len() > MAX_RECORDS_PER_ID {
                return Err("profile count limit".into());
            }
        }
        files.sort();
        Ok(files)
    }
    pub fn save(&self, d: &Draft) -> Result<PathBuf, String> {
        let text = encode(d)?;
        let directory = self.directory(d.context.source.id, true)?;
        if Self::files(&directory)?.len() >= MAX_RECORDS_PER_ID {
            return Err("profile count limit; explicit cleanup required".into());
        }
        let files = Self::files(&directory)?;
        let next = files
            .last()
            .map(|path| sequence(path))
            .transpose()?
            .unwrap_or(0)
            .checked_add(1)
            .ok_or("record sequence overflow")?;
        let name = format!("profile-{next:020}");
        let temp = directory.join(format!("{name}.tmp"));
        let target = directory.join(format!("{name}.srnp"));
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp)
            .map_err(|e| e.to_string())?;
        if let Err(error) = file
            .write_all(text.as_bytes())
            .and_then(|_| file.sync_all())
        {
            let _ = fs::remove_file(&temp);
            return Err(error.to_string());
        }
        drop(file);
        if let Err(error) = fs::rename(&temp, &target) {
            let _ = fs::remove_file(&temp);
            return Err(error.to_string());
        }
        Ok(target)
    }
    pub fn load(&self, context: &Context) -> Result<Draft, String> {
        let directory = self.directory(context.source.id, false)?;
        let files = Self::files(&directory)?;
        let path = files.last().ok_or("no stored draft for this ID")?;
        let file = fs::File::open(path).map_err(|e| e.to_string())?;
        if file.metadata().map_err(|e| e.to_string())?.len() > MAX_RECORD_BYTES as u64 {
            return Err("record size limit".into());
        }
        let mut text = String::new();
        file.take(MAX_RECORD_BYTES as u64 + 1)
            .read_to_string(&mut text)
            .map_err(|e| e.to_string())?;
        let d = decode(&text)?;
        resume(&d, context.clone())?;
        Ok(d)
    }
}
fn reject_link(path: &Path) -> Result<(), String> {
    if fs::symlink_metadata(path)
        .map_err(|e| e.to_string())?
        .file_type()
        .is_symlink()
    {
        return Err("store symlink rejected".into());
    }
    Ok(())
}

fn sequence(path: &Path) -> Result<u64, String> {
    let name = path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("invalid store filename")?;
    let value: u64 = name
        .strip_prefix("profile-")
        .and_then(|n| n.strip_suffix(".srnp"))
        .ok_or("invalid store filename")?
        .parse()
        .map_err(|_| "invalid store sequence")?;
    if value == 0 || name != format!("profile-{value:020}.srnp") {
        return Err("noncanonical store sequence".into());
    }
    Ok(value)
}
