use std::{
    fmt::Display,
    fs::File,
    io::{BufRead, BufReader},
};

use env_logger::Env;
use proglog::ProgLogBuilder;
use rayon::prelude::{ParallelBridge, ParallelIterator};

#[derive(Debug, Clone)]
struct LineNumber(u64);

impl Display for LineNumber {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "LineNumber {}", self.0)
    }
}

#[derive(Debug, Clone)]
struct Line {
    number: LineNumber,
    line: String,
}

/// This example reads all the lines from a text file and processes them with rayon.
/// Processing in this case is splitting into fields, parsing the floats, and summing them.
///
/// Suggested input data: https://archive.ics.uci.edu/ml/machine-learning-databases/00347/all_train.csv.gz (decompress it)
fn main() {
    env_logger::Builder::from_env(Env::default().default_filter_or("info")).init();
    let journal = ProgLogBuilder::new()
        .name("rayon-ex")
        .noun("records")
        .verb("Processed")
        .unit(1_000_000)
        .level(log::Level::Info)
        .build();

    let records = std::env::args()
        .skip(1)
        .collect::<Vec<_>>()
        .get(0)
        .cloned()
        .expect("Missing text arg.");

    let reader = BufReader::new(File::open(records).expect("Failed to open file"));

    let total: f64 = reader
        .lines()
        .skip(1)
        .enumerate()
        .map(|(i, line)| Line {
            number: LineNumber(i as u64),
            line: line.expect("Failed to read line"),
        })
        .par_bridge()
        .map(|line: Line| {
            let total: f64 = line
                .line
                .split(',')
                .map(|chunk| chunk.parse::<f64>().expect("Failed parse"))
                .sum();
            journal.record_with(|| &line.number);
            total
        })
        .sum();

    journal.flush(); // Not technically needed
    println!("Total = {}", total);
}
