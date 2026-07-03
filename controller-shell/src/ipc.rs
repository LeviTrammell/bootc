//! IPC re-exports — the wire-level types live in the shared cs-proto
//! crate so the nav adapter and the shell can't drift. Keep `parse` and
//! `Command` re-exported here as a stable surface for the rest of the
//! shell code (small change-radius if we later swap protocol crates).

pub use cs_proto::ShellCommand as Command;

pub fn parse(line: &str) -> Option<Command> {
    cs_proto::parse(line)
}
