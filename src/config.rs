use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{Context, Result, bail};
use globset::{Glob, GlobSet, GlobSetBuilder};
use serde::{Deserialize, Deserializer, de};

const DEFAULT_SLOW_THRESHOLD: Duration = Duration::from_secs(1);
const DEFAULT_VERY_SLOW_THRESHOLD: Duration = Duration::from_secs(5);
const CONFIG_PATHS: [&str; 2] = ["tester.toml", ".cargo/tester.toml"];

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct TesterConfig {
    pub execution: ExecutionConfig,
    pub groups: BTreeMap<String, GroupConfig>,
    pub history: HistoryConfig,
    pub output: OutputConfig,
    pub privacy: PrivacyConfig,
    pub timing: TimingConfig,
}

impl TesterConfig {
    pub fn load() -> Result<Self> {
        Self::load_from_root(".")
    }

    pub fn load_from_root(root: impl AsRef<Path>) -> Result<Self> {
        let root = root.as_ref();

        for relative_path in CONFIG_PATHS {
            let path = root.join(relative_path);
            if path.exists() {
                return Self::load_from(path);
            }
        }

        Ok(Self::default())
    }

    pub fn load_from(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::default());
        }

        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        Self::parse(&contents).with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn parse(contents: &str) -> Result<Self> {
        let config: Self = toml::from_str(contents)?;
        config.validate()?;
        Ok(config)
    }

    pub fn for_plain_file(&self) -> Self {
        let mut config = self.clone();
        config.output.color = false;
        config.output.emoji = false;
        config.output.unicode = false;
        config
    }

    pub fn apply_ci_defaults(&mut self) {
        self.output.color = false;
        self.output.emoji = false;
        self.output.unicode = false;
        self.privacy.include_captured_output = false;
        self.privacy.redact = true;
        self.privacy.include_source_context = false;
    }

    pub fn group(&self, name: &str) -> Result<&GroupConfig> {
        self.groups.get(name).ok_or_else(|| {
            let available = self.groups.keys().cloned().collect::<Vec<_>>().join(", ");
            if available.is_empty() {
                anyhow::anyhow!("group `{name}` is not configured")
            } else {
                anyhow::anyhow!("unknown group `{name}`; available groups: {available}")
            }
        })
    }

    pub fn is_sequential_test(&self, full_name: &str) -> Result<bool> {
        for group in self.groups.values().filter(|group| group.sequential) {
            if group.matches(full_name)? {
                return Ok(true);
            }
        }
        Ok(false)
    }

    pub fn sequential_test_names<'a>(
        &self,
        names: impl IntoIterator<Item = &'a str>,
    ) -> Result<HashSet<String>> {
        let matchers = self
            .groups
            .values()
            .filter(|group| group.sequential)
            .map(GroupConfig::matcher)
            .collect::<Result<Vec<_>>>()?;

        Ok(names
            .into_iter()
            .filter(|name| matchers.iter().any(|matcher| matcher.is_match(name)))
            .map(str::to_owned)
            .collect())
    }

    fn validate(&self) -> Result<()> {
        if self.output.output_path.as_os_str().is_empty() {
            bail!("output-path must not be empty");
        }

        if self.timing.slow_threshold > self.timing.very_slow_threshold {
            bail!("slow-threshold must be less than or equal to very-slow-threshold");
        }

        if self.output.max_width.is_some_and(|width| width < 60) {
            bail!("max-width must be at least 60 columns");
        }

        if self.history.max_runs == 0 || self.history.show_runs_history == 0 {
            bail!("history max-runs and show-runs-history must be greater than zero");
        }

        if self.execution.test_timeout.is_zero() || self.execution.discovery_timeout.is_zero() {
            bail!("execution timeouts must be greater than zero");
        }

        if self.execution.max_output_bytes == 0 || self.execution.max_discovery_output_bytes == 0 {
            bail!("execution output limits must be greater than zero");
        }

        if !self.history.regression_threshold_percent.is_finite()
            || self.history.regression_threshold_percent < 0.0
        {
            bail!("regression-threshold-percent must be finite and non-negative");
        }

        for (name, group) in &self.groups {
            if group.patterns.is_empty() {
                bail!("group `{name}` must define at least one pattern");
            }
            group
                .matcher()
                .with_context(|| format!("invalid pattern in group `{name}`"))?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct OutputConfig {
    pub color: bool,
    pub emoji: bool,
    pub max_width: Option<u16>,
    pub show_failed: bool,
    pub show_ignored: bool,
    pub show_passed: bool,
    pub unicode: bool,
    pub output_path: PathBuf,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            color: true,
            emoji: false,
            max_width: None,
            show_failed: true,
            show_ignored: true,
            show_passed: true,
            unicode: true,
            output_path: PathBuf::from(".cargo/tester-output/"),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct ExecutionConfig {
    pub jobs: usize,
    pub max_discovery_output_bytes: usize,
    pub max_output_bytes: usize,
    #[serde(deserialize_with = "deserialize_duration")]
    discovery_timeout: Duration,
    #[serde(deserialize_with = "deserialize_duration")]
    test_timeout: Duration,
}

impl ExecutionConfig {
    pub fn resolved_jobs(&self) -> usize {
        if self.jobs == 0 {
            std::thread::available_parallelism()
                .map(usize::from)
                .unwrap_or(1)
        } else {
            self.jobs
        }
    }

    pub fn discovery_timeout(&self) -> Duration {
        self.discovery_timeout
    }

    pub fn test_timeout(&self) -> Duration {
        self.test_timeout
    }
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        Self {
            jobs: 0,
            max_discovery_output_bytes: 16 * 1024 * 1024,
            max_output_bytes: 256 * 1024,
            discovery_timeout: Duration::from_secs(15 * 60),
            test_timeout: Duration::from_secs(5 * 60),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct PrivacyConfig {
    pub include_captured_output: bool,
    pub include_source_context: bool,
    pub redact: bool,
}

impl Default for PrivacyConfig {
    fn default() -> Self {
        Self {
            include_captured_output: true,
            include_source_context: true,
            redact: false,
        }
    }
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct GroupConfig {
    pub patterns: Vec<String>,
    pub sequential: bool,
}

impl GroupConfig {
    pub fn matches(&self, full_name: &str) -> Result<bool> {
        Ok(self.matcher()?.is_match(full_name))
    }

    pub(crate) fn matcher(&self) -> Result<GlobSet> {
        let mut builder = GlobSetBuilder::new();
        for pattern in &self.patterns {
            builder.add(Glob::new(pattern)?);
        }
        Ok(builder.build()?)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct HistoryConfig {
    pub enabled: bool,
    pub max_runs: usize,
    #[serde(alias = "show-runs")]
    pub show_runs_history: usize,
    pub regression_threshold_percent: f64,
    #[serde(deserialize_with = "deserialize_duration")]
    minimum_test_duration: Duration,
}

impl HistoryConfig {
    pub fn minimum_test_duration(&self) -> Duration {
        self.minimum_test_duration
    }
}

impl Default for HistoryConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            max_runs: 50,
            show_runs_history: 10,
            regression_threshold_percent: 50.0,
            minimum_test_duration: Duration::from_millis(100),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(default, rename_all = "kebab-case", deny_unknown_fields)]
pub struct TimingConfig {
    #[serde(deserialize_with = "deserialize_duration")]
    slow_threshold: Duration,
    #[serde(deserialize_with = "deserialize_duration")]
    very_slow_threshold: Duration,
}

impl TimingConfig {
    pub fn slow_threshold(&self) -> Duration {
        self.slow_threshold
    }

    pub fn very_slow_threshold(&self) -> Duration {
        self.very_slow_threshold
    }
}

impl Default for TimingConfig {
    fn default() -> Self {
        Self {
            slow_threshold: DEFAULT_SLOW_THRESHOLD,
            very_slow_threshold: DEFAULT_VERY_SLOW_THRESHOLD,
        }
    }
}

fn deserialize_duration<'de, D>(deserializer: D) -> Result<Duration, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    parse_duration(&value).map_err(de::Error::custom)
}

fn parse_duration(value: &str) -> std::result::Result<Duration, String> {
    let trimmed = value.trim();
    let (number, multiplier) = if let Some(number) = trimmed.strip_suffix("ms") {
        (number, 0.001)
    } else if let Some(number) = trimmed.strip_suffix('s') {
        (number, 1.0)
    } else if let Some(number) = trimmed.strip_suffix('m') {
        (number, 60.0)
    } else {
        (trimmed, 1.0)
    };
    let amount = number
        .trim()
        .parse::<f64>()
        .map_err(|_| format!("invalid duration `{value}`; use values such as 250ms, 1s, or 2m"))?;
    let seconds = amount * multiplier;

    Duration::try_from_secs_f64(seconds).map_err(|_| {
        format!("invalid duration `{value}`; duration must be finite and non-negative")
    })
}
