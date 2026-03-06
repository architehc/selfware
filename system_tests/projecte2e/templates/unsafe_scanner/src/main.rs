use unsafe_scanner::{Scanner, ScanResult};

fn main() {
    let mut scanner = Scanner::new(64);
    scanner.add_pattern("password");
    scanner.add_pattern("secret");
    scanner.add_pattern("token");

    let input = "The password is secret123 and the api_token is xyz";
    let results = scanner.scan(input);

    println!("Scan results for input ({} bytes):", input.len());
    for result in &results {
        println!(
            "  Found '{}' at offset {}",
            result.pattern, result.offset
        );
    }
    println!("Total matches: {}", results.len());
}
