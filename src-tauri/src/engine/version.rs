use semver::{Version, VersionReq};

/// The tested engine range — the single source of truth (README points
/// here); changing it is deliberate, never a drive-by. The floor is
/// 0.5.4, the first engine that answers `neurobrix info --json`;
/// 0.5.3 cannot be probed at all.
pub const SUPPORTED_ENGINE_RANGE: &str = ">=0.5.4,<0.6";

#[derive(Debug, PartialEq)]
pub enum VersionCheck {
    Compatible,
    Incompatible,
}

#[derive(Debug, PartialEq)]
pub enum VersionError {
    Malformed,
}

pub fn check_supported(engine_version: &str) -> Result<VersionCheck, VersionError> {
    let version: Version = engine_version
        .parse()
        .map_err(|_| VersionError::Malformed)?;
    let requirement: VersionReq = VersionReq::parse(SUPPORTED_ENGINE_RANGE)
        .expect("SUPPORTED_ENGINE_RANGE must be a valid requirement");
    if requirement.matches(&version) {
        Ok(VersionCheck::Compatible)
    } else {
        Ok(VersionCheck::Incompatible)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_inside_the_range_are_compatible() {
        for version in ["0.5.4", "0.5.5"] {
            assert_eq!(
                check_supported(version),
                Ok(VersionCheck::Compatible),
                "{version}"
            );
        }
    }

    #[test]
    fn versions_outside_the_range_are_incompatible() {
        // 0.5.3 predates `info --json`; it cannot answer discovery.
        // Pre-releases of an out-of-range version are excluded too: an
        // untested 0.6.0 candidate is not a tested engine.
        for version in ["0.5.2", "0.5.3", "0.6.0", "0.6.0-rc.1", "1.0.0"] {
            assert_eq!(
                check_supported(version),
                Ok(VersionCheck::Incompatible),
                "{version}"
            );
        }
    }

    #[test]
    fn non_semver_versions_are_malformed() {
        for version in ["", "0.5", "v0.5.4", "not.a.version"] {
            assert_eq!(
                check_supported(version),
                Err(VersionError::Malformed),
                "{version}"
            );
        }
    }
}
