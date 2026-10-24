//! p63-sim: Interactive CLI debugger and simulator for Paradox63

fn main() -> Result<(), anyhow::Error> {
    println!("Paradox63 CPU");
    println!("Type 'help' for commands, 'exit' to quit");

    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
