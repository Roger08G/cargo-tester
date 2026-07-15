use std::process::ExitCode;

fn main() -> ExitCode {
    match cargo_tester::app::run() {
        Ok(exit_code) => exit_code,
        Err(error) => {
            eprintln!("error: {error:#}");
            ExitCode::FAILURE
        }
    }
}
