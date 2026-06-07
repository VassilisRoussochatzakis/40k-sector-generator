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

#[cfg(test)]
mod tests {
    use super::*;

    /// `std::process::ExitCode` implements neither `PartialEq` nor a public
    /// getter, and its `Debug` is platform-specific. So we compare the `{:?}` of
    /// the produced code against the `{:?}` of an `ExitCode::from(expected)`
    /// built the same way — this stays correct across Unix/Windows because both
    /// sides go through the identical constructor.
    fn assert_code(err: &SectorError, expected: u8) {
        assert_eq!(
            format!("{:?}", from_error(err)),
            format!("{:?}", ExitCode::from(expected)),
            "wrong exit code for {err:?}",
        );
    }

    #[test]
    fn from_error_maps_each_variant_to_its_stable_code() {
        // ValidationFailed -> 1 (generic check failure).
        assert_code(
            &SectorError::ValidationFailed {
                error_count: 1,
                warning_count: 0,
            },
            1,
        );
        // GenerationCancelled -> 130 (POSIX SIGINT).
        assert_code(&SectorError::GenerationCancelled, 130);
        // Io -> 74 (EX_IOERR). `std::io::Error::other` is stable.
        assert_code(&SectorError::io("p", std::io::Error::other("x")), 74);
        // ConfigParse -> 78 (EX_CONFIG).
        assert_code(&SectorError::config_parse("p", "m"), 78);
        // InvalidConfig shares the 78 bucket with ConfigParse.
        assert_code(&SectorError::InvalidConfig("m".into()), 78);
        // NoWorldCandidates -> 65 (EX_DATAERR). This unit variant covers the
        // same code bucket as WorldDataLoad, which is awkward to construct
        // (needs a WorldError), so we exercise 65 through it.
        assert_code(&SectorError::NoWorldCandidates, 65);
        // ExportFailed -> 74 (output IO failure).
        assert_code(&SectorError::export("p", "m"), 74);
        // WeightedSelectionFailed -> 70 (EX_SOFTWARE).
        assert_code(
            &SectorError::WeightedSelectionFailed {
                context: "c".into(),
            },
            70,
        );
    }
}
