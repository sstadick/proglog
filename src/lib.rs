//! This is a simple, thread-safe, count-based, progress logger.
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
use log::{log, Level};
use std::{
    fmt::Display,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc,
    },
};

static DEFAULT_NAME: &str = "proglog";
static DEFAULT_NOUN: &str = "records";
static DEFAULT_VERB: &str = "Processed";
static DEFAULT_UNIT: u64 = 100_000;
static DEFAULT_LEVEL: Level = Level::Info;

pub struct ProgLog {
    counter: Arc<AtomicU64>,
    name: String,
    noun: String,
    verb: String,
    unit: u64,
    level: Level,
}

impl Default for ProgLog {
    fn default() -> Self {
        Self {
            counter: Default::default(),
            name: String::from(DEFAULT_NAME),
            noun: String::from(DEFAULT_NOUN),
            verb: String::from(DEFAULT_VERB),
            unit: DEFAULT_UNIT,
            level: DEFAULT_LEVEL,
        }
    }
}

impl ProgLog {
    pub fn new(name: String, noun: String, verb: String, unit: u64, level: Level) -> Self {
        Self {
            counter: Arc::new(AtomicU64::new(0)),
            name,
            noun,
            verb,
            unit,
            level,
        }
    }

    pub fn seen(&self) -> u64 {
        self.counter.load(Ordering::Relaxed)
    }

    pub fn record(&self) {
        let prev = self.counter.fetch_add(1, Ordering::Relaxed);
        let total = prev + 1;
        if total % self.unit == 0 {
            log!(
                self.level,
                "[{name}] {verb} {seen} {noun}",
                name = &self.name,
                verb = &self.verb,
                seen = total,
                noun = &self.noun
            )
        }
    }

    pub fn record_with<T, F>(&self, f: F)
    where
        F: Fn() -> T,
        T: Display,
    {
        let prev = self.counter.fetch_add(1, Ordering::Relaxed);
        let total = prev + 1;
        if total % self.unit == 0 {
            log!(
                self.level,
                "[{name}] {verb} {seen} {noun}: {extra}",
                name = &self.name,
                verb = &self.verb,
                seen = total,
                noun = &self.noun,
                extra = f()
            )
        }
    }

    pub fn flush_with<T, F>(&self, f: F)
    where
        F: Fn() -> T,
        T: Display,
    {
        let total = self.counter.load(Ordering::Relaxed);
        if total % self.unit != 0 {
            log!(
                self.level,
                "[{name}] {verb} {seen} {noun}: {extra}",
                name = &self.name,
                verb = &self.verb,
                seen = total,
                noun = &self.noun,
                extra = f()
            )
        }
    }

    pub fn flush(&self) {
        let total = self.counter.load(Ordering::Relaxed);
        if total % self.unit != 0 {
            log!(
                self.level,
                "[{name}] {verb} {seen} {noun}",
                name = &self.name,
                verb = &self.verb,
                seen = total,
                noun = &self.noun
            )
        }
    }
}

impl Drop for ProgLog {
    fn drop(&mut self) {
        self.flush();
    }
}

pub struct ProgLogBuilder {
    name: String,
    noun: String,
    verb: String,
    unit: u64,
    level: Level,
}

impl ProgLogBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn name(mut self, name: String) -> Self {
        self.name = name;
        self
    }

    pub fn noun(mut self, noun: String) -> Self {
        self.noun = noun;
        self
    }

    pub fn verb(mut self, verb: String) -> Self {
        self.verb = verb;
        self
    }

    pub fn unit(mut self, unit: u64) -> Self {
        self.unit = unit;
        self
    }

    pub fn level(mut self, level: Level) -> Self {
        self.level = level;
        self
    }

    pub fn build(self) -> ProgLog {
        ProgLog::new(self.name, self.noun, self.verb, self.unit, self.level)
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
    }

    fn test_simple_case(logger: &mut Logger) {
        let my_logger = ProgLogBuilder::new().build();
        for _i in 0..101 {
            my_logger.record()
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
}
