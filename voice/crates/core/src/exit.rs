/// Voice process exit codes.
///
/// Each code maps to exactly one `exit_reason` value and one Harmony action.
/// The binary `crates/voice` owns selection of the code; this enum carries the semantics.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExitCode {
    /// Agent completed; run acceptance; → reviewing
    Completed = 0,
    /// Agent failed mid-run; Harmony will retry with backoff
    Failed = 1,
    /// Hard abort (bad env, worktree setup failure, invalid manifest)
    HardAbort = 2,
    /// Agent called `infeasible`; spec needs reshaping
    Infeasible = 3,
    /// Agent called `needs_input`; ticket → awaiting_input
    NeedsInput = 4,
    /// Run cancelled via SIGTERM; ticket resets to ready
    Cancelled = 5,
}

impl ExitCode {
    pub fn as_i32(self) -> i32 {
        self as i32
    }

    /// True if the worktree should be kept (human inspection needed).
    pub fn keep_worktree(self) -> bool {
        matches!(self, Self::Completed | Self::Infeasible | Self::NeedsInput)
    }

    /// True if a full report is required (not optional or partial).
    pub fn report_required(self) -> bool {
        matches!(self, Self::Completed | Self::Infeasible | Self::NeedsInput)
    }

    pub fn exit_reason(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::HardAbort => "hard-abort",
            Self::Infeasible => "infeasible",
            Self::NeedsInput => "needs-input",
            Self::Cancelled => "cancelled",
        }
    }
}
