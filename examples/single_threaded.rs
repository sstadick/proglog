use env_logger::Env;
use proglog::ProgLogBuilder;

fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();

    let journal = ProgLogBuilder::new()
        .name("simple-ex")
        .noun("records")
        .verb("Processed")
        .unit(100)
        .level(log::Level::Info)
        .build();

    for i in 0..1000 {
        journal.record_with(|| i);
    }
}
