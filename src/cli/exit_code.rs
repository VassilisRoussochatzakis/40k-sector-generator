//! Map [`SectorError`] variants to stable [`ExitCode`] values for the CLI.
//!
//! Mapping (matches BSD sysexits-style conventions where reasonable):
//!
//! | Error variant                        | Exit code | Why                          |
//! |--------------------------------------|-----------|------------------------------|
//! | `ValidationFailed`                   | 1         | generic check failure        |
//! | `GenerationCancelled`                | 130       | POSIX SIGINT (Ctrl-C)        |
//! | `Io { .. }`                          | 74        | sysexits EX_IOERR            |
//! | `ConfigParse { .. }`, `InvalidConfig`| 78        | sysexits EX_CONFIG           |
//! | `WorldDataLoad { .. }`               | 65        | sysexits EX_DATAERR          |
//! | `NoWorldCandidates`                  | 65        | data error: empty pool       |
//! | `WeightedSelectionFailed`            | 70        | sysexits EX_SOFTWARE         |
//! | `ExportFailed`                       | 74        | output IO failure            |
//! | (other)                              | 70        | generic software fault       |

use std::process::ExitCode;

use sectorforge::SectorError;

/// Translate a [`SectorError`] into a stable process exit code.
#[must_use]
pub fn from_error(err: &SectorError) -> ExitCode {
    let code: u8 = match err {
        SectorError::ValidationFailed { .. } => 1,
        SectorError::GenerationCancelled => 130,
        SectorError::Io { .. } => 74,
        SectorError::ConfigParse { .. } | SectorError::InvalidConfig(_) => 78,
        SectorError::WorldDataLoad { .. } | SectorError::NoWorldCandidates => 65,
        SectorError::ExportFailed { .. } => 74,
        SectorError::WeightedSelectionFailed { .. } => 70,
        _ => 70,
    };
    ExitCode::from(code)
}
