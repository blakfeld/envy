/// Signals that the process should exit with the given code without printing an
/// additional error message. Used by commands that already printed their own
/// diagnostic output (e.g. `envy check`) and just need a non-zero exit code.
#[derive(Debug)]
pub struct SilentExit(pub i32);

impl std::fmt::Display for SilentExit {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "exit code {}", self.0)
    }
}

impl std::error::Error for SilentExit {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silent_exit_displays_code() {
        assert_eq!(format!("{}", SilentExit(1)), "exit code 1");
        assert_eq!(format!("{}", SilentExit(42)), "exit code 42");
    }

    #[test]
    fn silent_exit_code_zero_displays() {
        assert_eq!(format!("{}", SilentExit(0)), "exit code 0");
    }
}
