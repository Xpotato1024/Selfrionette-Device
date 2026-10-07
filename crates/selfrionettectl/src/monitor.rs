use std::io::{self, Write};

use selfrionette_core::DeviceId;
use selfrionette_serial::{DEFAULT_MAX_LINES, DeviceSession, SerialPortTransport};

use crate::CliError;

pub fn run(args: &[String]) -> Result<(), CliError> {
    let (port, expected_id, samples) = parse_args(args)?;
    let transport =
        SerialPortTransport::open(port).map_err(|error| CliError::Failure(error.to_string()))?;
    let mut output = io::stdout().lock();
    DeviceSession::new(transport)
        .monitor(expected_id, samples, |info, frame| {
            writeln!(output, "identity={} {frame:?}", info.identity)?;
            output.flush()
        })
        .map_err(|error| CliError::Failure(format!("monitor failed: {error}")))?;
    Ok(())
}

fn parse_args(args: &[String]) -> Result<(&str, DeviceId, usize), CliError> {
    let usage = || {
        CliError::Usage(
            "monitor requires exactly --port <port> --id <device-id> --samples <1..64>".to_owned(),
        )
    };
    let [
        port_flag,
        port,
        id_flag,
        id_text,
        samples_flag,
        samples_text,
    ] = args
    else {
        return Err(usage());
    };
    if port_flag != "--port"
        || port.is_empty()
        || port.starts_with("--")
        || id_flag != "--id"
        || samples_flag != "--samples"
    {
        return Err(usage());
    }
    let expected_id = id_text
        .parse::<DeviceId>()
        .map_err(|error| CliError::Usage(format!("invalid monitor device id: {error}")))?;
    let samples = samples_text.parse::<usize>().map_err(|_| usage())?;
    if samples == 0 || samples > DEFAULT_MAX_LINES {
        return Err(usage());
    }
    Ok((port, expected_id, samples))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "srn-0123456789abcdef0123456789abcdef";

    fn args(port: &str, id: &str, count: &str) -> Vec<String> {
        ["--port", port, "--id", id, "--samples", count]
            .into_iter()
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn monitor_validates_arguments_before_open() {
        for input in [
            vec![],
            args("", ID, "1"),
            args("--id", ID, "1"),
            args("COM5", "unprovisioned", "1"),
            args("COM5", "srn-00000000000000000000000000000000", "1"),
            args("COM5", ID, "0"),
            args("COM5", ID, "65"),
            args("COM5", ID, "-1"),
            args("COM5", ID, "NaN"),
            args("COM5", ID, "18446744073709551616"),
        ] {
            // Calling run is safe only for invalid inputs: all fail before open.
            assert!(matches!(run(&input), Err(CliError::Usage(_))));
        }
    }

    #[test]
    fn monitor_requires_all_flags_and_rejects_extras() {
        let valid = args("COM5", ID, "2");
        for index in [0, 2, 4] {
            let mut wrong = valid.clone();
            wrong[index] = "--unknown".to_owned();
            assert!(matches!(parse_args(&wrong), Err(CliError::Usage(_))));
        }
        let mut extra = valid.clone();
        extra.push("--yes".to_owned());
        assert!(matches!(parse_args(&extra), Err(CliError::Usage(_))));
        assert!(matches!(parse_args(&valid[..4]), Err(CliError::Usage(_))));
    }

    #[test]
    fn monitor_accepts_explicit_identity_and_bounded_counts() {
        for count in ["1", "64"] {
            let input = args("COM5", ID, count);
            let (port, id, samples) = parse_args(&input).expect("valid args");
            assert_eq!(port, "COM5");
            assert_eq!(id.to_string(), ID);
            assert_eq!(samples, count.parse::<usize>().unwrap());
        }
    }
}
