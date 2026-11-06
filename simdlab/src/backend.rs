//! Experiment backends: description, runtime detection and selection.

use std::fmt;

/// Static description of one backend.
#[derive(Debug)]
pub struct BackendInfo {
    /// Name used on the command line and in result files (`avx2`, `neon`, `scalar`, ...).
    pub name: &'static str,
    /// CPU features the backend needs, as [`crate::cpu`] names. Empty for scalar/portable code.
    pub requires: &'static [&'static str],
    /// One line for `--help` and `--list-backends`.
    pub summary: &'static str,
}

/// The backends of one experiment, implemented by its `Backend` enum.
///
/// Only [`Backends::all`], [`Backends::info`] and [`Backends::REFERENCE`] are implemented per
/// experiment; detection and selection are shared.
pub trait Backends: Copy + Eq + fmt::Debug + Send + Sync + 'static {
    /// The backend every other backend is tested against (normally needs no CPU features).
    const REFERENCE: Self;

    /// Every backend compiled for this target architecture, best first.
    fn all() -> &'static [Self];

    fn info(self) -> &'static BackendInfo;

    fn name(self) -> &'static str {
        self.info().name
    }

    /// Whether this CPU can run the backend.
    fn is_supported(self) -> bool {
        crate::cpu::has_all(self.info().requires)
    }

    /// The required features this CPU lacks.
    fn missing_features(self) -> Vec<&'static str> {
        crate::cpu::missing(self.info().requires)
    }

    /// Every backend this CPU supports, best first.
    fn available() -> Vec<Self> {
        Self::all().iter().copied().filter(|b| b.is_supported()).collect()
    }

    /// The best backend this CPU supports.
    fn best() -> Self {
        Self::all()
            .iter()
            .copied()
            .find(|b| b.is_supported())
            .expect("no supported backend (the reference backend should need no CPU features)")
    }

    /// The backend called `name`, if it is compiled for this architecture and this CPU supports it.
    fn from_name(name: &str) -> Result<Self, BackendError> {
        let b = Self::all().iter().copied().find(|b| b.name() == name).ok_or_else(|| BackendError::Unknown {
            name: name.to_string(),
            known: Self::all().iter().map(|b| b.name()).collect(),
        })?;
        match b.missing_features() {
            missing if missing.is_empty() => Ok(b),
            missing => Err(BackendError::Unsupported { name: b.name(), missing }),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BackendError {
    /// No backend of that name is compiled for this architecture.
    Unknown { name: String, known: Vec<&'static str> },
    /// The backend exists but this CPU lacks features it needs.
    Unsupported { name: &'static str, missing: Vec<&'static str> },
}

impl fmt::Display for BackendError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            BackendError::Unknown { name, known } => {
                write!(f, "unknown backend '{name}' (backends on this architecture: {})", known.join(", "))
            }
            BackendError::Unsupported { name, missing } => {
                write!(f, "backend '{name}' is not supported by this CPU (missing: {})", missing.join(", "))
            }
        }
    }
}

impl std::error::Error for BackendError {}

/// One line per backend: name, whether this CPU supports it, what it needs, summary. The default
/// (best supported) backend is marked with `*`; the reference with `(reference)`.
pub fn table<B: Backends>() -> String {
    let best = B::best();
    let width = B::all().iter().map(|b| b.name().len()).max().unwrap_or(0);
    let mut out = String::new();
    for &b in B::all() {
        let info = b.info();
        let missing = b.missing_features();
        let status = if missing.is_empty() { "yes" } else { "no" };
        let mark = if b == best { "*" } else { " " };
        let needs = if info.requires.is_empty() { "-".to_string() } else { info.requires.join(" ") };
        out += &format!("{mark} {:<width$}  {status:<3}  {needs}", info.name);
        if !missing.is_empty() {
            out += &format!(" (missing: {})", missing.join(" "));
        }
        if b == B::REFERENCE {
            out += "  (reference)";
        }
        out += &format!("\n    {}\n", info.summary);
    }
    out
}
