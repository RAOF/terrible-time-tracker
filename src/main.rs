use std::error::Error;

use time_tracker::parse_timelog;

fn main() -> Result<(), Box<dyn Error>> {
    let input = std::fs::read_to_string("timelog.txt")?;
    let entries = parse_timelog(&input)?;

    println!("Parsed {} entries.\n", entries.len());
    for entry in &entries {
        println!(
            "{} | category={:<14} | non_work={:<5} | tags={:?}\n    {}",
            entry.date.format("%Y-%m-%d %H:%M"),
            format!("{:?}", entry.category),
            entry.non_work,
            entry.tags,
            entry.description,
        );
    }

    Ok(())
}
