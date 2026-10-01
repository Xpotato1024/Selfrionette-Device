use std::env;
use std::process::ExitCode;

use selfrionette_core::{
    DeviceId, DeviceIdentity, HostCommand, ProtocolFrame, parse_line,
};

fn usage() -> &'static str {
    "usage:
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

fn print_frame(frame: &ProtocolFrame) {
    match frame {
        ProtocolFrame::Device(info) => {
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
            println!("unknown prefix={} fields={:?}", unknown.prefix, unknown.fields);
        }
    }
}

enum CliError {
    Usage(String),
    Failure(String),
}
