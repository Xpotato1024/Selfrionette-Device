//! Bounded, versioned binding manifests. Separate from synthetic .srnp previews.
//! No neutral, fitted coefficients, certification or readiness is restored.
use crate::input_calibration::InputQuality;
use crate::persistence::{MAX_RECORD_BYTES, hex, unhex};
use crate::setup::*;
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};

fn checked<T, E: std::fmt::Display>(result: Result<T, E>) -> Result<T, String> {
    result.map_err(|e| e.to_string())
}
fn number<T: std::str::FromStr>(s: &str) -> Result<T, String> {
    s.parse().map_err(|_| "invalid number".into())
}
fn flag(s: &str) -> Result<bool, String> {
    match s {
        "0" => Ok(false),
        "1" => Ok(true),
        _ => Err("invalid flag".into()),
    }
}
fn quality_name(q: InputQuality) -> &'static str {
    match q {
        InputQuality::Usable => "usable",
        InputQuality::Unknown => "unknown",
        InputQuality::Reset => "reset",
        InputQuality::Timeout => "timeout",
        InputQuality::Spike => "spike",
        InputQuality::Clipped => "clipped",
    }
}
fn quality(s: &str) -> Result<InputQuality, String> {
    match s {
        "usable" => Ok(InputQuality::Usable),
        "unknown" => Ok(InputQuality::Unknown),
        "reset" => Ok(InputQuality::Reset),
        "timeout" => Ok(InputQuality::Timeout),
        "spike" => Ok(InputQuality::Spike),
        "clipped" => Ok(InputQuality::Clipped),
        _ => Err("invalid quality".into()),
    }
}
pub fn domain_name(domain: SampleDomain) -> &'static str {
    match domain {
        SampleDomain::RawAdc => "raw_adc",
        SampleDomain::LegacyFirmwareV2 => "firmware_v2_vector",
        SampleDomain::DeviceOrientedCounts => "device_oriented_counts",
        SampleDomain::Newtons => "newtons",
        SampleDomain::NormalizedInput => "normalized_input",
    }
}
fn domain(s: &str) -> Result<SampleDomain, String> {
    match s {
        "raw_adc" => Ok(SampleDomain::RawAdc),
        "firmware_v2_vector" => Ok(SampleDomain::LegacyFirmwareV2),
        "device_oriented_counts" => Ok(SampleDomain::DeviceOrientedCounts),
        "newtons" => Ok(SampleDomain::Newtons),
        "normalized_input" => Ok(SampleDomain::NormalizedInput),
        _ => Err("invalid sample domain".into()),
    }
}
fn encode_evidence(e: &Evidence) -> Result<String, String> {
    checked(e.validate())?;
    Ok(format!(
        "evidence,{},{},{}\n",
        match e.origin {
            Origin::Synthetic => "synthetic",
            Origin::Recorded => "recorded",
        },
        quality_name(e.quality),
        hex(&e.reference)?
    ))
}
fn fields<'a>(line: &'a str, tag: &str, count: usize) -> Result<Vec<&'a str>, String> {
    let f: Vec<_> = line.split(',').collect();
    if f.len() != count || f[0] != tag {
        return Err(format!("invalid {tag} row"));
    }
    Ok(f)
}
fn decode_evidence(line: &str) -> Result<Evidence, String> {
    let f = fields(line, "evidence", 4)?;
    let e = Evidence {
        origin: match f[1] {
            "synthetic" => Origin::Synthetic,
            "recorded" => Origin::Recorded,
            _ => return Err("invalid provenance".into()),
        },
        quality: quality(f[2])?,
        reference: unhex(f[3])?,
    };
    checked(e.validate())?;
    Ok(e)
}
fn optional_text(s: &str) -> Result<Option<String>, String> {
    if s == "?" {
        Ok(None)
    } else {
        unhex(s).map(Some)
    }
}
fn encode_optional(s: &Option<String>) -> Result<String, String> {
    s.as_ref().map_or_else(|| Ok("?".into()), |s| hex(s))
}
fn optional_number<T: std::str::FromStr>(s: &str) -> Result<Option<T>, String> {
    if s == "?" {
        Ok(None)
    } else {
        number(s).map(Some)
    }
}

fn encode_setup(s: &DeviceSetup) -> Result<String, String> {
    checked(s.validate())?;
    let mut out = format!(
        "srn-setup,1\ndevice,{},{},{},{},{}\n{}",
        s.device_id,
        s.assembly_revision,
        s.config_revision,
        s.settings_generation,
        u8::from(s.mapping_validated),
        encode_evidence(&s.evidence)?
    );
    for (wire, ch) in s.channels.iter().enumerate() {
        out.push_str(&format!(
            "wire,{wire},{},{},{},{}\n",
            ch.logical_axis
                .map_or_else(|| "?".into(), |v| v.to_string()),
            encode_optional(&ch.location)?,
            encode_optional(&ch.positive_direction)?,
            ch.polarity.map_or_else(|| "?".into(), |v| v.to_string())
        ));
    }
    Ok(out)
}
fn decode_setup(lines: &[&str]) -> Result<DeviceSetup, String> {
    if lines.len() != 3 + N || lines[0] != "srn-setup,1" {
        return Err("invalid setup schema/rows".into());
    }
    let d = fields(lines[1], "device", 6)?;
    let mut channels = Vec::with_capacity(N);
    for wire in 0..N {
        let c = fields(lines[3 + wire], "wire", 6)?;
        if c[1] != wire.to_string() {
            return Err("wire order/count rejected".into());
        }
        channels.push(ChannelOrientation {
            logical_axis: optional_number(c[2])?,
            location: optional_text(c[3])?,
            positive_direction: optional_text(c[4])?,
            polarity: optional_number(c[5])?,
        });
    }
    let s = DeviceSetup {
        device_id: checked(d[1].parse())?,
        assembly_revision: number(d[2])?,
        config_revision: number(d[3])?,
        settings_generation: number(d[4])?,
        mapping_validated: flag(d[5])?,
        evidence: decode_evidence(lines[2])?,
        channels: channels.try_into().map_err(|_| "channel count")?,
    };
    checked(s.validate())?;
    Ok(s)
}
fn encode_context_tail(f: &FirmwareBinding, o: &OperatorContext) -> Result<String, String> {
    checked(f.validate())?;
    checked(o.validate())?;
    Ok(format!(
        "firmware,{},{},{}\noperator,{},{},{}\n",
        hex(&f.version)?,
        f.source_revision,
        domain_name(f.sample_domain),
        hex(&o.operator)?,
        hex(&o.posture)?,
        hex(&o.grip)?
    ))
}
fn decode_context_tail(lines: &[&str]) -> Result<(FirmwareBinding, OperatorContext), String> {
    let f = fields(lines[0], "firmware", 4)?;
    let o = fields(lines[1], "operator", 4)?;
    let firmware = FirmwareBinding {
        version: unhex(f[1])?,
        source_revision: f[2].into(),
        sample_domain: domain(f[3])?,
    };
    let operator = OperatorContext {
        operator: unhex(o[1])?,
        posture: unhex(o[2])?,
        grip: unhex(o[3])?,
    };
    checked(firmware.validate())?;
    checked(operator.validate())?;
    Ok((firmware, operator))
}
fn rows<'a>(text: &'a str, header: &str, count: usize) -> Result<Vec<&'a str>, String> {
    if text.len() > MAX_RECORD_BYTES {
        return Err("record size limit".into());
    }
    let lines: Vec<_> = text.lines().collect();
    if lines.len() != count || lines[0] != header {
        return Err("schema/row count rejected".into());
    }
    Ok(lines)
}
pub fn encode_context(c: &CurrentContext) -> Result<String, String> {
    checked(c.validate())?;
    Ok(format!(
        "srn-setup-context,1\n{}{}",
        encode_setup(&c.setup)?,
        encode_context_tail(&c.firmware, &c.operator)?
    ))
}
pub fn decode_context(text: &str) -> Result<CurrentContext, String> {
    let lines = rows(text, "srn-setup-context,1", N + 6)?;
    let (firmware, operator) = decode_context_tail(&lines[N + 4..])?;
    let c = CurrentContext {
        setup: decode_setup(&lines[1..N + 4])?,
        firmware,
        operator,
    };
    checked(c.validate())?;
    Ok(c)
}
pub fn encode_binding(p: &ProfileBinding) -> Result<String, String> {
    checked(p.validate())?;
    Ok(format!(
        "srn-profile-binding,1\n{}{}profile,{},{}\n{}",
        encode_setup(&p.setup)?,
        encode_context_tail(&p.firmware, &p.operator)?,
        p.profile_revision,
        hex(&p.parameter_record)?,
        encode_evidence(&p.evidence)?
    ))
}
pub fn decode_binding(text: &str) -> Result<ProfileBinding, String> {
    let lines = rows(text, "srn-profile-binding,1", N + 8)?;
    let (firmware, operator) = decode_context_tail(&lines[N + 4..N + 6])?;
    let f = fields(lines[N + 6], "profile", 3)?;
    let p = ProfileBinding {
        setup: decode_setup(&lines[1..N + 4])?,
        firmware,
        operator,
        profile_revision: number(f[1])?,
        parameter_record: unhex(f[2])?,
        evidence: decode_evidence(lines[N + 7])?,
    };
    // Reload checks the same invariants as creation, without promoting provenance.
    checked(p.validate())?;
    Ok(p)
}
fn read(path: &Path) -> Result<String, String> {
    let metadata = checked(fs::symlink_metadata(path))?;
    if !metadata.is_file() || metadata.len() > MAX_RECORD_BYTES as u64 {
        return Err("regular file <=32768 bytes required".into());
    }
    let file = checked(fs::File::open(path))?;
    let mut text = String::new();
    checked(
        file.take(MAX_RECORD_BYTES as u64 + 1)
            .read_to_string(&mut text),
    )?;
    if text.len() > MAX_RECORD_BYTES {
        return Err("record size limit".into());
    }
    Ok(text)
}
pub fn load_context(path: &Path) -> Result<CurrentContext, String> {
    decode_context(&read(path)?)
}
pub fn load_binding(path: &Path) -> Result<ProfileBinding, String> {
    decode_binding(&read(path)?)
}

/// Immutable save to an explicit existing private directory; never replaces a file.
/// Concurrent readers may reject an incomplete write. No neutral/readiness is saved.
pub fn save_binding_new(path: &Path, binding: &ProfileBinding) -> Result<(), String> {
    if !path.is_absolute() {
        return Err("absolute binding path required".into());
    }
    let text = encode_binding(binding)?;
    if text.len() > MAX_RECORD_BYTES {
        return Err("record size limit".into());
    }
    let mut file = checked(
        fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path),
    )?;
    let result = file
        .write_all(text.as_bytes())
        .and_then(|_| file.sync_all());
    drop(file);
    if let Err(e) = result {
        let _ = fs::remove_file(path);
        return Err(e.to_string());
    }
    Ok(())
}
