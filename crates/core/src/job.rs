//! Job state exposed to the UI. A batch of files is several jobs.

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::ConversionError;
use crate::tool::ConversionOutcome;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(tag = "state", rename_all = "camelCase")]
#[ts(export)]
pub enum JobStatus {
    Queued,
    Running { progress: f32 },
    Succeeded { outcome: ConversionOutcome },
    Failed { error: ConversionError },
    Cancelled,
}

impl JobStatus {
    /// `true` once the job can no longer change.
    pub fn is_finished(&self) -> bool {
        matches!(
            self,
            Self::Succeeded { .. } | Self::Failed { .. } | Self::Cancelled
        )
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export)]
pub struct Job {
    pub id: String,
    pub tool_id: String,
    /// Input file this job works on.
    pub input: String,
    pub status: JobStatus,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finished_states() {
        assert!(!JobStatus::Queued.is_finished());
        assert!(!JobStatus::Running { progress: 0.5 }.is_finished());
        assert!(JobStatus::Cancelled.is_finished());
        assert!(JobStatus::Failed { error: ConversionError::Corrupt }.is_finished());
    }
}
