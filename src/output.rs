use colored::Colorize;

pub fn header(msg: &str) {
    println!("\n{}", msg.bold());
}

pub fn step(msg: &str) {
    println!("  {} {}", "→".blue().bold(), msg);
}

pub fn success(msg: &str) {
    println!("  {} {}", "✓".green().bold(), msg);
}

pub fn skip(msg: &str) {
    println!("  {} {}", "○".dimmed(), msg.dimmed());
}

pub fn info(msg: &str) {
    println!("  {} {}", "·".cyan(), msg);
}
