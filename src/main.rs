//! `patchproof-verify CERT.json` — check a certificate without trusting its author.

use std::process::ExitCode;

use patchproof_verify::{replay_bound, Status};

const USAGE: &str = "\
patchproof-verify — independently re-check a patchproof elimination certificate.

USAGE:
    patchproof-verify <CERTIFICATE.json>
    patchproof-verify -            # read the certificate from stdin

EXIT STATUS:
    0   VERIFIED    the arithmetic replays AND the constraints are the claimed class's
    1   UNVERIFIED  the arithmetic replays but nothing binds it to a claim (not a pass)
    1   REJECTED    the certificate does not check out
    2   the file could not be read

There is no solver here, and no dependency on the Python package that produced the
certificate. That is the point: a proof you can only check with the prover's own code
is not much of a proof.
";

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 1 || args[0] == "-h" || args[0] == "--help" {
        eprint!("{}", USAGE);
        return ExitCode::from(if args.len() == 1 { 0 } else { 2 });
    }

    let raw = if args[0] == "-" {
        std::io::read_to_string(std::io::stdin()).unwrap_or_default()
    } else {
        match std::fs::read_to_string(&args[0]) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("patchproof-verify: cannot read {:?}: {}", args[0], e);
                return ExitCode::from(2);
            }
        }
    };

    let cert: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("REJECTED   certificate is not valid JSON: {}", e);
            return ExitCode::from(1);
        }
    };

    let (status, message) = replay_bound(&cert);
    // VERIFIED on stdout so a pipeline can consume it; anything else on stderr so it
    // cannot be mistaken for a result.
    if status == Status::Verified {
        println!("{:<10} {}", status, message);
    } else {
        eprintln!("{:<10} {}", status, message);
    }
    ExitCode::from(status.exit_code() as u8)
}
