//! `aeternavault-cli`: AeternaVault on the command line. See `docs/cli-windows.md`
//! and `docs/cli-linux.md`, or run `aeternavault-cli --help`.

fn main() -> std::process::ExitCode {
    aeterna_vault::cli::main()
}
