pub mod shadowenv;

pub use shadowenv::Shadowenv;

use anyhow::Result;
use std::collections::HashMap;
use std::path::Path;

pub trait EnvManager {
    fn name(&self) -> &str;
    fn setup(
        &self,
        dir: &Path,
        vars: &HashMap<String, String>,
        path_prepends: &[String],
    ) -> Result<()>;
}
