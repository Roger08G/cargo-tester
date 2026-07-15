use std::env;

use anyhow::{Result, anyhow, bail};

const CARGO_FLAG_OPTIONS: &[&str] = &[
    "--workspace",
    "--all",
    "--lib",
    "--bins",
    "--tests",
    "--examples",
    "--benches",
    "--all-targets",
    "--release",
    "--all-features",
    "--no-default-features",
    "--locked",
    "--offline",
    "--frozen",
    "--ignore-rust-version",
];
const CARGO_VALUE_OPTIONS: &[&str] = &[
    "-p",
    "--package",
    "--exclude",
    "--features",
    "-j",
    "--jobs",
    "--target",
    "--target-dir",
    "--manifest-path",
    "--config",
    "--profile",
    "--bin",
    "--test",
    "--example",
    "--bench",
];

#[derive(Debug, PartialEq, Eq)]
pub struct Cli {
    pub cargo_args: Vec<String>,
    pub ci: bool,
    pub details: bool,
    pub failed: bool,
    pub filter: Option<String>,
    pub group: Option<String>,
    pub harness_args: Vec<String>,
    pub help: bool,
    pub history: bool,
    pub last_failed: bool,
    pub no_color: bool,
    pub version: bool,
}

impl Cli {
    pub fn parse() -> Result<Self> {
        Self::parse_from(env::args().skip(1))
    }

    pub fn parse_from<I, S>(args: I) -> Result<Self>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut args = args.into_iter().map(Into::into).peekable();
        if args.peek().is_some_and(|arg| arg == "tester") {
            args.next();
        }

        let mut cli = Self {
            cargo_args: Vec::new(),
            ci: false,
            details: false,
            failed: false,
            filter: None,
            group: None,
            harness_args: Vec::new(),
            help: false,
            history: false,
            last_failed: false,
            no_color: false,
            version: false,
        };

        while let Some(arg) = args.next() {
            match arg.as_str() {
                "--" => {
                    cli.harness_args.extend(args);
                    break;
                }
                "-h" | "--help" => cli.help = true,
                "--ci" => cli.ci = true,
                "--details" => cli.details = true,
                "--failed" => cli.failed = true,
                "--history" => cli.history = true,
                "--last-failed" => cli.last_failed = true,
                "--no-color" => cli.no_color = true,
                "-V" | "--version" => cli.version = true,
                "--group" => {
                    if cli.group.is_some() {
                        bail!("option --group can only be used once");
                    }
                    cli.group = Some(next_value(&mut args, "--group")?);
                }
                value if value.starts_with("--group=") => {
                    if cli.group.is_some() {
                        bail!("option --group can only be used once");
                    }
                    cli.group = Some(inline_value(value, "--group")?);
                }
                value if CARGO_FLAG_OPTIONS.contains(&value) => {
                    cli.cargo_args.push(value.to_owned());
                }
                value if CARGO_VALUE_OPTIONS.contains(&value) => {
                    cli.cargo_args.push(value.to_owned());
                    cli.cargo_args.push(next_value(&mut args, value)?);
                }
                value if is_inline_cargo_option(value) => {
                    cli.cargo_args.push(value.to_owned());
                }
                unknown if unknown.starts_with('-') => bail!("unknown option: {unknown}"),
                value if cli.filter.is_none() => cli.filter = Some(value.to_owned()),
                value => bail!("unexpected extra argument: {value}"),
            }
        }

        validate_harness_args(&cli.harness_args)?;
        cli.validate()?;
        Ok(cli)
    }

    fn validate(&self) -> Result<()> {
        if self.history
            && (self.details
                || self.failed
                || self.last_failed
                || self.filter.is_some()
                || self.group.is_some()
                || !self.cargo_args.is_empty()
                || !self.harness_args.is_empty())
        {
            bail!("--history cannot be combined with test execution options");
        }

        Ok(())
    }
}

fn next_value<I>(args: &mut I, option: &str) -> Result<String>
where
    I: Iterator<Item = String>,
{
    args.next()
        .filter(|value| !value.is_empty() && !value.starts_with('-'))
        .ok_or_else(|| anyhow!("option {option} requires a value"))
}

fn inline_value(value: &str, option: &str) -> Result<String> {
    value
        .split_once('=')
        .map(|(_, value)| value.to_owned())
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow!("option {option} requires a value"))
}

fn is_inline_cargo_option(value: &str) -> bool {
    CARGO_VALUE_OPTIONS
        .iter()
        .filter(|option| option.starts_with("--"))
        .any(|option| value.starts_with(&format!("{option}=")))
        || (value.starts_with("-j") && value.len() > 2)
}

fn validate_harness_args(args: &[String]) -> Result<()> {
    const RESERVED: &[&str] = &["--exact", "--list", "--skip"];

    if let Some(argument) = args.iter().find(|argument| {
        RESERVED.contains(&argument.as_str())
            || argument.starts_with("--skip=")
            || argument.starts_with("--format")
            || argument.starts_with("--color")
    }) {
        bail!("test harness option {argument} is managed by cargo-tester");
    }

    Ok(())
}

pub fn print_help() {
    println!(
        "\
cargo-tester

Usage:
  cargo tester [OPTIONS] [CARGO OPTIONS] [FILTER] [-- HARNESS OPTIONS]

Options:
  --details        Print failed test details and save details.json
  --failed         Show only failed tests in the table
  --last-failed    Run only tests that failed in the previous execution
  --group <NAME>   Run a group configured in tester.toml
  --ci             Use stable CI output and always write details.json
  --history        Show recent executions without running tests
  --no-color       Disable colored output
  -h, --help       Print help
  -V, --version    Print version

Examples:
  cargo tester
  cargo tester --details --failed
  cargo tester --last-failed
  cargo tester --group parser
  cargo tester --ci --workspace --all-features
  cargo tester --history
"
    );
}
