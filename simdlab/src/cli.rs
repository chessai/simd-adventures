//! Command-line arguments shared by every experiment binary (clap).
//!
//! ```ignore
//! #[derive(clap::Parser)]
//! struct Cli {
//!     #[command(flatten)]
//!     backend: simdlab::cli::BackendArg<mycrate::Backend>,   // --backend NAME, --list-backends
//!     #[arg(long, value_delimiter = ',', value_parser = simdlab::cli::parse_size)]
//!     sizes: Vec<usize>,
//! }
//! ```

use crate::backend::{self, Backends};
use clap::builder::{PossibleValue, TypedValueParser};
use clap::error::ErrorKind;
use clap::{Arg, ArgAction, ArgMatches, Args, Command, FromArgMatches};
use std::ffi::OsStr;
use std::marker::PhantomData;

/// Environment variable that sets `--backend` when the flag is not given.
pub const BACKEND_ENV: &str = "SIMDLAB_BACKEND";

/// `--backend NAME`: one backend, by default the best this CPU supports. Also adds
/// `--list-backends`, which prints [`backend::table`] and exits.
#[derive(Clone, Copy, Debug)]
pub struct BackendArg<B> {
    pub backend: B,
}

/// `--backend NAME[,NAME...]` (alias `--backends`): several backends, by default every backend
/// this CPU supports. For tools that compare backends in one run. Also adds `--list-backends`.
#[derive(Clone, Debug)]
pub struct BackendsArg<B> {
    pub backends: Vec<B>,
    /// The list came from `--backend` / `SIMDLAB_BACKEND` rather than the default.
    pub explicit: bool,
}

/// Parses a backend name: unknown names and backends this CPU does not support are errors.
#[derive(Clone, Copy)]
pub struct BackendParser<B>(PhantomData<B>);

impl<B> Default for BackendParser<B> {
    fn default() -> Self {
        BackendParser(PhantomData)
    }
}

impl<B: Backends> TypedValueParser for BackendParser<B> {
    type Value = B;

    fn parse_ref(&self, cmd: &Command, _arg: Option<&Arg>, value: &OsStr) -> Result<B, clap::Error> {
        let name = value.to_str().ok_or_else(|| clap::Error::new(ErrorKind::InvalidUtf8).with_cmd(cmd))?;
        B::from_name(name).map_err(|e| clap::Error::raw(ErrorKind::InvalidValue, format!("{e}\n")).with_cmd(cmd))
    }

    fn possible_values(&self) -> Option<Box<dyn Iterator<Item = PossibleValue> + '_>> {
        Some(Box::new(B::all().iter().map(|&b| {
            let missing = b.missing_features();
            let reference = if b == B::REFERENCE { " (reference)" } else { "" };
            let status = if missing.is_empty() { String::new() } else { format!(" [not supported: missing {}]", missing.join(" ")) };
            PossibleValue::new(b.name()).help(format!("{}{reference}{status}", b.info().summary))
        })))
    }
}

fn backend_arg<B: Backends>() -> Arg {
    Arg::new("backend").long("backend").value_name("NAME").env(BACKEND_ENV).value_parser(BackendParser::<B>::default())
}

/// `--list-backends`: prints [`backend::table`] and exits while the arguments are being parsed,
/// so it works even when the command has required arguments.
#[derive(Clone, Copy)]
struct ListBackends<B>(PhantomData<B>);

impl<B: Backends> TypedValueParser for ListBackends<B> {
    type Value = bool;

    fn parse_ref(&self, cmd: &Command, arg: Option<&Arg>, value: &OsStr) -> Result<bool, clap::Error> {
        let on = clap::builder::BoolishValueParser::new().parse_ref(cmd, arg, value)?;
        if on {
            print!("{}", backend::table::<B>());
            std::process::exit(0);
        }
        Ok(false)
    }
}

fn list_arg<B: Backends>() -> Arg {
    Arg::new("list-backends")
        .long("list-backends")
        .action(ArgAction::SetTrue)
        .value_parser(ListBackends::<B>(PhantomData))
        .help("List the backends compiled for this architecture, whether this CPU supports them, and exit")
}

impl<B: Backends> Args for BackendArg<B> {
    fn augment_args(cmd: Command) -> Command {
        cmd.arg(backend_arg::<B>().help("SIMD backend [default: the best one this CPU supports]")).arg(list_arg::<B>())
    }
    fn augment_args_for_update(cmd: Command) -> Command {
        Self::augment_args(cmd)
    }
}

impl<B: Backends> FromArgMatches for BackendArg<B> {
    fn from_arg_matches(m: &ArgMatches) -> Result<Self, clap::Error> {
        Ok(BackendArg { backend: m.get_one::<B>("backend").copied().unwrap_or_else(B::best) })
    }
    fn update_from_arg_matches(&mut self, m: &ArgMatches) -> Result<(), clap::Error> {
        if let Some(&b) = m.get_one::<B>("backend") {
            self.backend = b;
        }
        Ok(())
    }
}

impl<B: Backends> Args for BackendsArg<B> {
    fn augment_args(cmd: Command) -> Command {
        cmd.arg(
            backend_arg::<B>()
                .visible_alias("backends")
                .value_name("NAME[,NAME...]")
                .action(ArgAction::Append)
                .value_delimiter(',')
                .help("SIMD backends to run [default: every backend this CPU supports]"),
        )
        .arg(list_arg::<B>())
    }
    fn augment_args_for_update(cmd: Command) -> Command {
        Self::augment_args(cmd)
    }
}

impl<B: Backends> FromArgMatches for BackendsArg<B> {
    fn from_arg_matches(m: &ArgMatches) -> Result<Self, clap::Error> {
        Ok(match m.get_many::<B>("backend") {
            Some(v) => BackendsArg { backends: v.copied().collect(), explicit: true },
            None => BackendsArg { backends: B::available(), explicit: false },
        })
    }
    fn update_from_arg_matches(&mut self, m: &ArgMatches) -> Result<(), clap::Error> {
        if let Some(v) = m.get_many::<B>("backend") {
            self.backends = v.copied().collect();
            self.explicit = true;
        }
        Ok(())
    }
}

/// Parses a byte or element count: `4096`, `32k`, `1.5M`, `1G` (binary multiples, any case).
/// Use as `value_parser = simdlab::cli::parse_size`, with `value_delimiter = ','` for lists.
pub fn parse_size(s: &str) -> Result<usize, String> {
    let s = s.trim();
    let (num, mult) = match s.as_bytes().last() {
        Some(b'k' | b'K') => (&s[..s.len() - 1], 1usize << 10),
        Some(b'm' | b'M') => (&s[..s.len() - 1], 1 << 20),
        Some(b'g' | b'G') => (&s[..s.len() - 1], 1 << 30),
        _ => (s, 1),
    };
    match num.parse::<f64>() {
        Ok(x) if x >= 0.0 && x.is_finite() => Ok((x * mult as f64) as usize),
        _ => Err(format!("invalid size '{s}' (expected e.g. 4096, 32k, 1.5M, 1G)")),
    }
}

#[cfg(test)]
mod tests {
    use super::parse_size;

    #[test]
    fn sizes() {
        assert_eq!(parse_size("4096"), Ok(4096));
        assert_eq!(parse_size("32k"), Ok(32 << 10));
        assert_eq!(parse_size("256K"), Ok(256 << 10));
        assert_eq!(parse_size("1.5M"), Ok(3 << 19));
        assert_eq!(parse_size("1G"), Ok(1 << 30));
        assert!(parse_size("x").is_err());
        assert!(parse_size("-1k").is_err());
    }
}
