use std::process::ExitCode;

fn main() -> ExitCode {
    let args = argh::cargo_from_env();

    // When successful, this does not return.
    let Err(error) = cargo_attach::attach(args);

    eprintln!("error: {error}");

    ExitCode::FAILURE
}
