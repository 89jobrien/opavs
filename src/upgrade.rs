use anyhow::{Context, Result};

const REPO_OWNER: &str = "89jobrien";
const REPO_NAME: &str = "opavs";
const BIN_NAME: &str = "opavs";

/// Result of checking GitHub Releases and applying the newest OPAVS version.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum UpgradeOutcome {
    /// The running executable was replaced with this version.
    Updated { version: String },
    /// The running executable already matches this version.
    UpToDate { version: String },
}

trait ReleaseUpdater {
    fn update(&self) -> Result<self_update::Status>;
}

struct GitHubReleaseUpdater;

impl ReleaseUpdater for GitHubReleaseUpdater {
    fn update(&self) -> Result<self_update::Status> {
        self_update::backends::github::Update::configure()
            .repo_owner(REPO_OWNER)
            .repo_name(REPO_NAME)
            .bin_name(BIN_NAME)
            .current_version(env!("CARGO_PKG_VERSION"))
            .show_download_progress(true)
            .no_confirm(true)
            .build()
            .context("configure GitHub release updater")?
            .update()
            .context("download and install latest OPAVS release")
    }
}

/// Download and install the newest compatible release from GitHub.
pub fn run() -> Result<UpgradeOutcome> {
    execute(&GitHubReleaseUpdater)
}

fn execute(updater: &impl ReleaseUpdater) -> Result<UpgradeOutcome> {
    match updater.update()? {
        self_update::Status::Updated(version) => Ok(UpgradeOutcome::Updated { version }),
        self_update::Status::UpToDate(version) => Ok(UpgradeOutcome::UpToDate { version }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeUpdater {
        status: self_update::Status,
    }

    impl ReleaseUpdater for FakeUpdater {
        fn update(&self) -> Result<self_update::Status> {
            Ok(self.status.clone())
        }
    }

    #[test]
    fn execute_reports_updated_version() {
        let outcome = execute(&FakeUpdater {
            status: self_update::Status::Updated("1.2.3".to_string()),
        })
        .expect("upgrade outcome");

        assert_eq!(
            outcome,
            UpgradeOutcome::Updated {
                version: "1.2.3".to_string()
            }
        );
    }

    #[test]
    fn execute_reports_current_version() {
        let outcome = execute(&FakeUpdater {
            status: self_update::Status::UpToDate("1.2.3".to_string()),
        })
        .expect("upgrade outcome");

        assert_eq!(
            outcome,
            UpgradeOutcome::UpToDate {
                version: "1.2.3".to_string()
            }
        );
    }
}
