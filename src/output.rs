use colored::Colorize;

#[cfg(test)]
pub static WARN_CALL_COUNT: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(0);

pub fn header(msg: &str) -> String {
    let s = format!("\n{}", msg.bold());
    println!("{s}");
    s
}

pub fn step(msg: &str) -> String {
    let s = format!("  {} {}", "→".blue().bold(), msg);
    println!("{s}");
    s
}

pub fn success(msg: &str) -> String {
    let s = format!("  {} {}", "✓".green().bold(), msg);
    println!("{s}");
    s
}

pub fn skip(msg: &str) -> String {
    let s = format!("  {} {}", "○".dimmed(), msg.dimmed());
    println!("{s}");
    s
}

pub fn info(msg: &str) -> String {
    let s = format!("  {} {}", "·".cyan(), msg);
    println!("{s}");
    s
}

pub fn warn(msg: &str) -> String {
    #[cfg(test)]
    WARN_CALL_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let s = format!("  {} {}", "!".yellow().bold(), msg);
    println!("{s}");
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn header_contains_message() {
        let s = header("hello world");
        assert!(s.contains("hello world"), "header output must contain the message: {s}");
        assert!(s.contains('\n'), "header output must start with a newline: {s}");
    }

    #[test]
    fn step_contains_message() {
        let s = step("doing something");
        assert!(s.contains("doing something"), "step output must contain the message: {s}");
    }

    #[test]
    fn success_contains_message() {
        let s = success("all good");
        assert!(s.contains("all good"), "success output must contain the message: {s}");
    }

    #[test]
    fn skip_contains_message() {
        let s = skip("skipping this");
        assert!(s.contains("skipping this"), "skip output must contain the message: {s}");
    }

    #[test]
    fn info_contains_message() {
        let s = info("some info");
        assert!(s.contains("some info"), "info output must contain the message: {s}");
    }

    #[test]
    fn warn_contains_message() {
        let s = warn("watch out");
        assert!(s.contains("watch out"), "warn output must contain the message: {s}");
    }

    #[test]
    fn header_different_from_step() {
        let h = header("msg");
        let st = step("msg");
        assert_ne!(h, st, "header and step must produce different output for the same message");
    }

    #[test]
    fn success_different_from_skip() {
        let s = success("msg");
        let sk = skip("msg");
        assert_ne!(s, sk);
    }
}
