use std::env;
use std::process::ExitCode;

use selfrionette_core::{
    DeviceId, DeviceIdentity, DeviceInfo, HostCommand, ProtocolFrame, parse_line,
};
use selfrionette_serial::{DeviceSession, SerialPortTransport, available_port_names};

fn usage() -> &'static str {
    "usage:
  selfrionettectl list
  selfrionettectl info --port <port>
  selfrionettectl provision --port <port> --id <device-id> --yes
  selfrionettectl parse-line <line>
  selfrionettectl validate-id <device-id>
  selfrionettectl encode info
  selfrionettectl encode tare
  selfrionettectl encode provision <device-id>"
}

fn main() -> ExitCode {
    match run(env::args().skip(1).collect()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(CliError::Usage(message)) => {
            eprintln!("{message}\n\n{}", usage());
            ExitCode::from(2)
        }
        Err(CliError::Failure(message)) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: Vec<String>) -> Result<(), CliError> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(CliError::Usage("missing command".to_owned()));
    };

    match command {
        "list" => run_list(&args[1..]),
        "info" => run_info(&args[1..]),
        "provision" => run_provision(&args[1..]),
        "parse-line" => {
            if args.len() != 2 {
                return Err(CliError::Usage(
                    "parse-line requires exactly one protocol line".to_owned(),
                ));
            }
            let frame = parse_line(&args[1])
                .map_err(|error| CliError::Failure(format!("parse failed: {error}")))?;
            print_frame(&frame);
            Ok(())
        }
        "validate-id" => {
            if args.len() != 2 {
                return Err(CliError::Usage(
                    "validate-id requires exactly one device id".to_owned(),
                ));
            }
            let device_id = args[1]
                .parse::<DeviceId>()
                .map_err(|error| CliError::Failure(format!("invalid device id: {error}")))?;
            println!("{device_id}");
            Ok(())
        }
        "encode" => encode_command(&args[1..]),
        other => Err(CliError::Usage(format!("unknown command: {other}"))),
    }
}

fn run_list(args: &[String]) -> Result<(), CliError> {
    if !args.is_empty() {
        return Err(CliError::Usage("list does not accept arguments".to_owned()));
    }

    let ports = available_port_names()
        .map_err(|error| CliError::Failure(format!("failed to enumerate serial ports: {error}")))?;
    for port in ports {
        println!("{port}");
    }
    Ok(())
}

fn run_info(args: &[String]) -> Result<(), CliError> {
    let port = parse_port_only(args, "info")?;
    let transport =
        SerialPortTransport::open(port).map_err(|error| CliError::Failure(error.to_string()))?;
    let mut session = DeviceSession::new(transport);
    let info = session
        .query_info()
        .map_err(|error| CliError::Failure(format!("info failed: {error}")))?;
    print_device_info(&info);
    Ok(())
}

fn run_provision(args: &[String]) -> Result<(), CliError> {
    let (port, device_id) = parse_provision_args(args)?;

    let transport = SerialPortTransport::open(port)
        .map_err(|error| CliError::Failure(error.to_string()))?;
    let mut session = DeviceSession::new(transport);
    let info = session
        .provision(device_id)
        .map_err(|error| CliError::Failure(format!("provision failed: {error}")))?;
    print_device_info(&info);
    Ok(())
}

fn parse_port_only<'a>(args: &'a [String], command: &str) -> Result<&'a str, CliError> {
    match args {
        [port_flag, port] if port_flag == "--port" && !port.is_empty() => Ok(port),
        _ => Err(CliError::Usage(format!(
            "{command} requires exactly --port <port>"
        ))),
    }
}

fn parse_provision_args(args: &[String]) -> Result<(&str, DeviceId), CliError> {
    let [port_flag, port, id_flag, id_text, yes_flag] = args else {
        return Err(CliError::Usage(
            "provision requires exactly --port <port> --id <device-id> --yes".to_owned(),
        ));
    };

    if port_flag != "--port" || id_flag != "--id" || yes_flag != "--yes" || port.is_empty() {
        return Err(CliError::Usage(
            "provision requires exactly --port <port> --id <device-id> --yes".to_owned(),
        ));
    }

    let device_id = id_text
        .parse::<DeviceId>()
        .map_err(|error| CliError::Failure(format!("invalid device id: {error}")))?;
    Ok((port, device_id))
}

fn encode_command(args: &[String]) -> Result<(), CliError> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(CliError::Usage("encode requires a command".to_owned()));
    };

    let encoded = match command {
        "info" if args.len() == 1 => HostCommand::Info.encode_line(),
        "tare" if args.len() == 1 => HostCommand::Tare.encode_line(),
        "provision" if args.len() == 2 => {
            let device_id = args[1]
                .parse::<DeviceId>()
                .map_err(|error| CliError::Failure(format!("invalid device id: {error}")))?;
            HostCommand::Provision(device_id).encode_line()
        }
        "info" | "tare" | "provision" => {
            return Err(CliError::Usage(format!(
                "wrong argument count for encode {command}"
            )));
        }
        other => {
            return Err(CliError::Usage(format!(
                "unknown protocol command for encode: {other}"
            )));
        }
    };

    print!("{encoded}");
    Ok(())
}

fn print_device_info(info: &DeviceInfo) {
    let identity = match &info.identity {
        DeviceIdentity::Provisioned(device_id) => device_id.to_string(),
        DeviceIdentity::Unprovisioned => "unprovisioned".to_owned(),
    };
    match info.validate_compatibility() {
        Ok(()) => println!(
            "device protocol={} firmware={} identity={} channels={} compatibility=ok",
            info.protocol_major, info.firmware_version, identity, info.channel_count
        ),
        Err(error) => println!(
            "device protocol={} firmware={} identity={} channels={} compatibility=error:{error}",
            info.protocol_major, info.firmware_version, identity, info.channel_count
        ),
    }
}

fn print_frame(frame: &ProtocolFrame) {
    match frame {
        ProtocolFrame::Device(info) => print_device_info(info),
        ProtocolFrame::Vector(vector) => {
            println!(
                "vector timestamp_ms={} channels={:?}",
                vector.timestamp_ms, vector.channels
            );
        }
        ProtocolFrame::Status(status) => {
            println!("status token={} args={:?}", status.token, status.args);
        }
        ProtocolFrame::Warning(warning) => {
            println!("warn token={} args={:?}", warning.token, warning.args);
        }
        ProtocolFrame::Unknown(unknown) => {
            println!(
                "unknown prefix={} fields={:?}",
                unknown.prefix, unknown.fields
            );
        }
    }
}

enum CliError {
    Usage(String),
    Failure(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_ID: &str = "srn-0123456789abcdef0123456789abcdef";

    #[test]
    fn provision_requires_yes_before_any_live_operation() {
        let args = vec![
            "--port".to_owned(),
            "COM5".to_owned(),
            "--id".to_owned(),
            VALID_ID.to_owned(),
        ];
        assert!(matches!(
            parse_provision_args(&args),
            Err(CliError::Usage(_))
        ));
    }

    #[test]
    fn parses_explicit_provision_arguments() {
        let args = vec![
            "--port".to_owned(),
            "COM5".to_owned(),
            "--id".to_owned(),
            VALID_ID.to_owned(),
            "--yes".to_owned(),
        ];
        let (port, id) = parse_provision_args(&args).expect("valid provision args");
        assert_eq!(port, "COM5");
        assert_eq!(id.to_string(), VALID_ID);
    }

    #[test]
    fn info_requires_explicit_port() {
        assert!(matches!(
            parse_port_only(&[], "info"),
            Err(CliError::Usage(_))
        ));
    }
}
