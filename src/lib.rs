//! This is a simple, thread-safe, count-based, progress logger.
//!
//! This progress logger is intended to be as low-overhead as possible so that it can be used in [hot-loops](#things-to-know).
//!
//! # Synopsis
//!
//! `proglog` hooks into your existing `log` implementation (i.e. `env_logger`) and will output a log message every `unit` number of items it has seen.
//! There are two primary methods, `record()` and `record_with(...)`.
//! `record()` simply increments the counter and will cause a log message to output when `counter % unit == 0`.
//! `record_with(Fn() -> impl Display)` takes a function that outputs anything implementing display which will be appended to the log message.
//!
//! # Things to Know
//!
//! If `unit` is too small, and your loop is too tight, this will output many log messages which will slow your program down in the same way any logging would slow a program down in a hot loop.
//! If `unit` is sufficiently large, this should be safe to put in a hot loop as all it does increment update an atomic `u64`.
//!
//! If your loop is tight, `unit` is small, _and_ you are using rayon / updating from multiple threads your log messages may end up out of order.
//! There is no guaranteed ordering of the submission of the log message to the logger.
//! So thread A could hit the first `unit` break, thread B could hit the second point at the same time, but thread B gets to submit its log message first.
//! Having sufficiently large `unit` will mitigate this, but you should not be depending on the log output order here.
//! The tradeoff made is for speed of incrementing so this can be put in hot loops over guaranteed output ordering.
//!
//! # Example
//!
//! ```rust
//! use proglog::ProgLogBuilder;
//!
//! // Note a `log` backend needs to be globally initialized first
//! env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
//!
//! let logger = ProgLogBuilder::new().build();
//! for i in 0..10_000 {
//!     logger.record_with(|| format!("Logged item: {}", i));
//! }
//! // The logger will flush when it is dropped, writing a final progress message no mater the count.
//! // Alternatively you can call .flush() or .flush_with().
//! ```
//!
//! # Features
//!
//! ## `pretty_counts`
//!
//! The `pretty_counts` features turns on the ability to format the numbers in the log messages.
//! Set the [`ProgLogBuilder::count_formatter`] to one of the [`CountFormatterKind`]s and numbers will
//! be formatted accordingly. i.e. `100000000` -> `100_000_000` with [`CountFormatterKind::Underscore`].
//! ```
#![deny(missing_docs, unsafe_code)]
use log::{log, Level};
use std::{
    fmt::Display,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};
#[cfg(feature = "pretty_counts")]
use thousands::{
    policies::{COMMA_SEPARATOR, DOT_SEPARATOR, HEX_FOUR, SPACE_SEPARATOR, UNDERSCORE_SEPARATOR},
    Separable,
};

static DEFAULT_NAME: &str = "proglog";
static DEFAULT_NOUN: &str = "records";
static DEFAULT_VERB: &str = "Processed";
static DEFAULT_UNIT: u64 = 100_000;
static DEFAULT_LEVEL: Level = Level::Info;

/// The types of formatting separators that can be applied to counts.
#[cfg(feature = "pretty_counts")]
pub enum CountFormatterKind {
    /// Delimit counter with a `,`.
    Comma,
    /// Delimit counter with a `.`.
    Dot,
    /// Delimit counter with a ` ` every four hexadecimal digits.
    HexFour,
    /// Delimit counter with a ` `.
    Space,
    /// Delimit counter with an `_`.
    Underscore,
    /// Don't delimit counter.
    Nothing,
}

#[cfg(feature = "pretty_counts")]
impl CountFormatterKind {
    fn fmt(&self, count: u64) -> String {
        match self {
            CountFormatterKind::Nothing => count.to_string(),
            CountFormatterKind::Comma => count.separate_by_policy(COMMA_SEPARATOR),
            CountFormatterKind::Dot => count.separate_by_policy(DOT_SEPARATOR),
            CountFormatterKind::HexFour => count.separate_by_policy(HEX_FOUR),
            CountFormatterKind::Space => count.separate_by_policy(SPACE_SEPARATOR),
            CountFormatterKind::Underscore => count.separate_by_policy(UNDERSCORE_SEPARATOR),
        }
    }
}

/// [`ProgLog`] is the the progress logger.
///
/// `ProgLog` hooks into your underlying logger implementation and will emit a
/// log message every time the counter hits a multiple of `unit` at the indicated
/// `level`.
///
/// There are two primary methods for incrementing the counter:
///
/// - [`ProgLog::record`]
/// - [`ProgLog::record_with`]
///
/// Both of these methods will increment the counter and check to see if a log
/// message should be emitted.
///
/// The structure of output messages will look like:
///
/// ```text
/// [{name}] {verb} {seen} {noun}: {meta}
/// ```
///
/// where `meta` is anything returned by the closure given to [`ProgLog::record_with`].
/// `seen` is the number of items counted so far.
///
/// A log message can be force-written by calling [`ProgLog::flush`]/[`ProgLog::flush_with`].
/// Calling flush does not end the logger, another log message will be written on drop.
/// Additionally, flush will be called on drop.
///
/// **Note**: `unit` should be adjusted so that you emit ~1 log message every 15 seconds.
/// If `unit` is too small and this is in a hot-loop logging will happen too frequently
/// and impact performance.
pub struct ProgLog {
    /// The counter tracks the number of items seen by the logger.
    counter: Arc<AtomicU64>,
    /// The name of the logger, used so that multiple progress loggers can run at once.
    name: String,
    /// The noun used in the log output string format, ideally lowercase and plural.
    noun: String,
    /// The verb used in the log output string format, ideally capitalized.
    verb: String,
    /// How many items must be seen before emitting a log message.
    unit: u64,
    /// The [`log::Level`] at which to emit log messages.
    level: Level,
    /// The formatter to use for outputting the current count.
    #[cfg(feature = "pretty_counts")]
    count_formatter: CountFormatterKind,
}

impl Default for ProgLog {
    /// Default for [`ProgLog`].
    fn default() -> Self {
        Self {
            counter: Default::default(),
            name: String::from(DEFAULT_NAME),
            noun: String::from(DEFAULT_NOUN),
            verb: String::from(DEFAULT_VERB),
            unit: DEFAULT_UNIT,
            level: DEFAULT_LEVEL,
            #[cfg(feature = "pretty_counts")]
            count_formatter: CountFormatterKind::Nothing,
        }
    }
}

impl ProgLog {
    /// Create a new [`ProgLog`].
    ///
    /// The [`ProgLogBuilder`] should be preferred.
    #[allow(clippy::must_use_candidate)]
    pub fn new(
        name: String,
        noun: String,
        verb: String,
        unit: u64,
        level: Level,
        #[cfg(feature = "pretty_counts")] count_formatter: CountFormatterKind,
    ) -> Self {
        Self {
            counter: Arc::new(AtomicU64::new(0)),
            name,
            noun,
            verb,
            unit,
            level,
            #[cfg(feature = "pretty_counts")]
            count_formatter,
        }
    }

    /// Get the number of items seen so far.
    ///
    /// This should be treated with some caution as it is using the
    /// atomic load with [`Ordering::Relaxed`].
    pub fn seen(&self) -> u64 {
        self.counter.load(Ordering::Relaxed)
    }

    /// Helper method to pull out log formatting .
    #[inline]
    fn log_it(&self, total: u64) {
        #[cfg(feature = "pretty_counts")]
        {
            log!(
                self.level,
                "[{name}] {verb} {seen} {noun}",
                name = &self.name,
                verb = &self.verb,
                seen = self.count_formatter.fmt(total),
                noun = &self.noun
            );
        }
        #[cfg(not(feature = "pretty_counts"))]
        {
            log!(
                self.level,
                "[{name}] {verb} {seen} {noun}",
                name = &self.name,
                verb = &self.verb,
                seen = total,
                noun = &self.noun
            );
        }
    }

    /// Helper method to pull out log formatting with custom user closure.
    #[inline]
    fn log_it_with<F, T>(&self, f: F, total: u64)
    where
        F: Fn() -> T,
        T: Display,
    {
        #[cfg(feature = "pretty_counts")]
        {
            log!(
                self.level,
                "[{name}] {verb} {seen} {noun}: {extra}",
                name = &self.name,
                verb = &self.verb,
                seen = self.count_formatter.fmt(total),
                noun = &self.noun,
                extra = f()
            );
        }

        #[cfg(not(feature = "pretty_counts"))]
        {
            log!(
                self.level,
                "[{name}] {verb} {seen} {noun}: {extra}",
                name = &self.name,
                verb = &self.verb,
                seen = total,
                noun = &self.noun,
                extra = f()
            );
        }
    }

    /// Increment the progress logger by 1 and check if a new message should be emitted.
    ///
    /// Returns `true` if total seen after incrementing is a multiple of `unit`.
    pub fn record(&self) -> bool {
        let prev = self.counter.fetch_add(1, Ordering::Relaxed);
        let total = prev + 1;
        if total % self.unit == 0 {
            self.log_it(total);
            true
        } else {
            false
        }
    }

    /// Increment the progress logger by 1 and check if a new message should be emitted.
    ///
    /// The returned displayable from the passed in closure will be appended to the log message.
    ///
    /// Returns `true` if total seen after incrementing is a multiple of `unit`.
    ///
    /// # Example
    ///
    /// ```rust
    /// use proglog::ProgLogBuilder;
    ///
    /// // Note a `log` backend needs to be globally initialized first
    /// env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    ///
    /// let logger = ProgLogBuilder::new().build();
    /// for i in 0..10_000 {
    ///     logger.record_with(|| format!("Logged item: {}", i));
    /// }
    /// // The logger will flush when it is dropped, writing a final progress message no mater the count.
    /// // Alternatively you can call .flush() or .flush_with().
    /// ```
    pub fn record_with<T, F>(&self, f: F) -> bool
    where
        F: Fn() -> T,
        T: Display,
    {
        let prev = self.counter.fetch_add(1, Ordering::Relaxed);
        let total = prev + 1;
        if total % self.unit == 0 {
            self.log_it_with(f, total);
            true
        } else {
            false
        }
    }

    /// Force the output of a log message, including the output of the input closure.
    ///
    /// This does not increment the counter.
    /// This does not close the logger.
    pub fn flush_with<T, F>(&self, f: F)
    where
        F: Fn() -> T,
        T: Display,
    {
        let total = self.counter.load(Ordering::Relaxed);
        if total % self.unit != 0 {
            self.log_it_with(f, total);
        }
    }

    /// Force the output of a log message.
    ///
    /// This does not increment the counter.
    /// This does not close the logger.
    pub fn flush(&self) {
        let total = self.counter.load(Ordering::Relaxed);
        if total % self.unit != 0 {
            self.log_it(total);
        }
    }
}

impl Drop for ProgLog {
    /// Drop the logger, calling flush before dropping.
    fn drop(&mut self) {
        self.flush();
    }
}

/// The builder for [`ProgLog`].
pub struct ProgLogBuilder {
    name: String,
    noun: String,
    verb: String,
    unit: u64,
    level: Level,
    /// The formatter to use for outputting the current count.
    #[cfg(feature = "pretty_counts")]
    count_formatter: CountFormatterKind,
}

impl ProgLogBuilder {
    /// Create a new [`ProgLogBuilder`].
    pub fn new() -> Self {
        Self::default()
    }

    /// The name of the logger, used so that multiple progress loggers can run at once.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = name.into();
        self
    }

    /// The noun used in the log output string format, ideally lowercase and plural.
    pub fn noun(mut self, noun: impl Into<String>) -> Self {
        self.noun = noun.into();
        self
    }

    /// The verb used in the log output string format, ideally capitalized.
    pub fn verb(mut self, verb: impl Into<String>) -> Self {
        self.verb = verb.into();
        self
    }

    /// How many items must be seen before emitting a log message.
    pub fn unit(mut self, unit: u64) -> Self {
        self.unit = unit;
        self
    }

    /// The [`log::Level`] at which to emit log messages.
    pub fn level(mut self, level: Level) -> Self {
        self.level = level;
        self
    }

    /// The formatter to use for outputting the current count.
    #[cfg(feature = "pretty_counts")]
    pub fn count_formatter(mut self, formatter: CountFormatterKind) -> Self {
        self.count_formatter = formatter;
        self
    }

    /// Build the [`ProgLog`] instance.
    pub fn build(self) -> ProgLog {
        ProgLog::new(
            self.name,
            self.noun,
            self.verb,
            self.unit,
            self.level,
            #[cfg(feature = "pretty_counts")]
            self.count_formatter,
        )
    }
}

impl Default for ProgLogBuilder {
    fn default() -> Self {
        Self {
            name: String::from(DEFAULT_NAME),
            noun: String::from(DEFAULT_NOUN),
            verb: String::from(DEFAULT_VERB),
            unit: DEFAULT_UNIT,
            level: DEFAULT_LEVEL,
            #[cfg(feature = "pretty_counts")]
            count_formatter: CountFormatterKind::Nothing,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use logtest::Logger;
    use rayon::prelude::*;

    fn drain_logger(logger: &mut Logger) {
        while let Some(_msg) = logger.pop() {}
    }

    /// This function drives all other tests since the logtest Logger is global :(.
    ///
    /// Each function called is expected to drain the logger.
    #[test]
    fn test_log_messages() {
        let mut logger = logtest::Logger::start();
        test_simple_case(&mut logger);
        test_rayon(&mut logger);
        test_messages_simple(&mut logger);
        assert_eq!(logger.len(), 0);
        test_messages_simple_verify_unit(&mut logger);
        assert_eq!(logger.len(), 0);
        test_messages_rayon(&mut logger);
        assert_eq!(logger.len(), 0);
        #[cfg(feature = "pretty_counts")]
        {
            test_pretty_counts(&mut logger);
            assert_eq!(logger.len(), 0);
        }
    }

    fn test_simple_case(logger: &mut Logger) {
        let my_logger = ProgLogBuilder::new().build();
        for _i in 0..101 {
            my_logger.record();
        }
        assert_eq!(my_logger.seen(), 101);
        drain_logger(logger);
    }

    fn test_rayon(logger: &mut Logger) {
        let my_logger = ProgLogBuilder::new().build();
        (0..1_000_000).par_bridge().for_each(|_i| {
            my_logger.record();
        });
        assert_eq!(my_logger.seen(), 1_000_000);
        drain_logger(logger);
    }

    fn test_messages_simple(logger: &mut Logger) {
        let my_logger = ProgLogBuilder::new().unit(1).build();
        my_logger.record_with(|| "This is a test");
        assert_eq!(logger.len(), 1);
        assert!(logger.pop().unwrap().args().ends_with("This is a test"));
        drain_logger(logger);
    }

    fn test_messages_simple_verify_unit(logger: &mut Logger) {
        let my_logger = ProgLogBuilder::new().unit(10).build();
        for _ in 0..9 {
            my_logger.record_with(|| "This is a test");
        }
        assert_eq!(logger.len(), 0);
        my_logger.record_with(|| "The 10th");
        assert_eq!(logger.len(), 1);
        assert!(logger.pop().unwrap().args().ends_with("The 10th"));
        drain_logger(logger)
    }

    fn test_messages_rayon(logger: &mut Logger) {
        let my_logger = ProgLogBuilder::new().unit(100_000).build();

        // Note - it just so happens the log messages are in the correct order here,
        // if the loop is tight enough, and the unit is too small, and depending how
        // rayon breaks things up the logger internal queue / print buffer can get
        // out of order.
        (1..=1_000_000).par_bridge().for_each(|i| {
            my_logger.record_with(|| format!("Logged {}", i));
        });
        assert_eq!(my_logger.seen(), 1_000_000);

        assert_eq!(logger.len(), 10);

        for msg in (100_000..=1_000_000).step_by(100_000) {
            let found = logger.pop().unwrap();
            assert!(found.args().ends_with(&msg.to_string()));
        }
        drain_logger(logger);
    }

    #[cfg(feature = "pretty_counts")]
    fn test_pretty_counts(logger: &mut Logger) {
        let my_logger = ProgLogBuilder::new()
            .unit(100_000)
            .count_formatter(CountFormatterKind::Underscore)
            .build();
        for _ in 0..99_999 {
            my_logger.record_with(|| "This is a test");
        }
        assert_eq!(logger.len(), 0);
        my_logger.record_with(|| "The 100,000th");
        assert_eq!(logger.len(), 1);
        assert_eq!(
            logger.pop().unwrap().args(),
            "[proglog] Processed 100_000 records: The 100,000th"
        );
        drain_logger(logger)
    }
}
