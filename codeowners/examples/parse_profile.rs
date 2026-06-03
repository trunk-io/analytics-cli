/// Profile where parse time goes in GitHubOwners::from_reader.
use std::{fs, io::BufReader, time::Instant};

use codeowners::{FromReader, GitHubOwners};

fn main() {
    let path = std::env::args().nth(1).expect("pass CODEOWNERS path");

    // Read file
    let t = Instant::now();
    let bytes = fs::read(&path).expect("read file");
    println!(
        "Read file:           {:.2?} ({} bytes)",
        t.elapsed(),
        bytes.len()
    );

    // Parse
    let t = Instant::now();
    let _owners = GitHubOwners::from_reader(BufReader::new(&bytes[..])).expect("parse");
    println!("from_reader total:   {:.2?}", t.elapsed());

    // Run again to warm up
    let t = Instant::now();
    for _ in 0..3 {
        let _ = GitHubOwners::from_reader(BufReader::new(&bytes[..])).expect("parse");
    }
    println!("from_reader x3 avg:  {:.2?}", t.elapsed() / 3);
}
