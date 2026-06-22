use std::path::PathBuf;

/// What the user chose. `main.rs` maps this to directive-file writes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    Cd(PathBuf),
    Open { path: PathBuf, cmd: String },
    Abort,
}

#[cfg(test)]
mod contract {
    use super::*;
    // --- Contract assertions ---
    #[test]
    fn outcome_shapes() {
        let p = PathBuf::from("/tmp");
        let _ = Outcome::Cd(p.clone());
        let _ = Outcome::Open { path: p, cmd: String::new() };
        let _ = Outcome::Abort;
    }
}
