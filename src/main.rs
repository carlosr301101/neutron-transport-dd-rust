//! `nts-rs` command line interface entry point.

use std::process::ExitCode;

fn main() -> ExitCode {
    neutron_transport_dd::cli::main()
}
