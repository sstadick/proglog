# 🦫 proglog

<p align="center">
  <a href="https://github.com/sstadick/proglog/actions?query=workflow%3Aci"><img src="https://github.com/sstadick/proglog/workflows/ci/badge.svg" alt="Build Status"></a>
  <img src="https://img.shields.io/crates/l/proglog.svg" alt="license">
  <a href="https://crates.io/crates/proglog"><img src="https://img.shields.io/crates/v/proglog.svg?colorB=319e8c" alt="Version info"></a><br>
</p>

[Documentation](https://docs.rs/proglog)
[Crates.io](https://crates.io/crates/proglog)

This is a simple, thread-safe, count-based, progress logger.

## Synopsis

`proglog` hooks into your existing `log` implementation (i.e. `env_logger`) and will output a log message every `unit` number of items it has seen.
There are two primary methods, `record()` and `record_with(...)`.
`record()` simply increments the counter and will cause a log message to output when `counter % unit == 0`.
`record_with(Fn() -> impl Display)` takes a function that outputs anything implementing display which will be appended to the log message.

## How to use this

Add to your deps:

```bash
cargo add proglog
```

## Usage

Please see the [rayon example](./examples/rayon.rs).

```rust
use proglog::ProgLogBuilder;

// Note a `log` backend needs to be globally initialized first
env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

let logger = ProgLogBuilder::new().build();
for i in 0..10_000 {
    logger.record_with(|| format!("Logged item: {}", i));
}

// The logger will flush when it is dropped, writing a final progress message no mater the count.
// Alternatively you can call .flush() or .flush_with().
```

## Things to know

If `unit` is too small, and your loop is too tight, this will output many log messages which will slow your program down in the same way any logging would slow a program down in a hot loop.
If `unit` is sufficiently large, this should be safe to put in a hot loop as all it does increment update an atomic `u64`.

If your loop is tight, `unit` is small, _and_ you are using rayon / updating from multiple threads your log messages may end up out of order.
There is no guaranteed ordering of the submission of the log message to the logger.
So thread A could hit the first `unit` break, thread B could hit the second point at the same time, but thread B gets to submit its log message first.
Having sufficiently large `unit` will mitigate this, but you should not be depending on the log output order here.
The tradeoff made is for speed of incrementing so this can be put in hot loops over guaranteed output ordering.

## Tests

```bash
cargo test
```

## Direct Inspirations

- The ProgressLogger found in [fgpyo](https://github.com/fulcrumgenomics/fgpyo/)
- The ProgressLogger in [fgbio](https://github.com/fulcrumgenomics/fgbio/)