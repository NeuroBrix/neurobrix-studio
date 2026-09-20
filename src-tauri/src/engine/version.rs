use semver::{Version, VersionReq};

/// The range of engine versions this Studio build has been tested against.
/// This constant is the single source of truth; README.md describes it and
/// points here. Changing it is a deliberate decision, never a drive-by.
pub const SUPPORTED_ENGINE_RANGE: &str = ">=0.5.3,<0.6";

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
