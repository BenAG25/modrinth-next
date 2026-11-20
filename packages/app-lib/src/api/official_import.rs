//! Modrinth Next: import data from the official Modrinth App on first start.

use crate::state::DirectoryInfo;
pub use crate::state::{OFFICIAL_APP_IDENTIFIER, OfficialAppData};

/// Returns the official app's data if it can be imported into this app.
pub fn find_importable(app_identifier: &str) -> Option<OfficialAppData> {
    DirectoryInfo::find_official_app_data(app_identifier)
}

/// Don't ask again.
pub fn decline(app_identifier: &str) {
    DirectoryInfo::decline_official_import(app_identifier)
}

/// Copies profiles, accounts and settings. Call before `State::init`.
pub async fn import(
    app_identifier: &str,
    data: &OfficialAppData,
) -> crate::Result<()> {
    DirectoryInfo::import_official_app_data(app_identifier, data).await
}
