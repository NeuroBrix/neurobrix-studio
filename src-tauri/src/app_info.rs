#[derive(serde::Serialize)]
pub(crate) struct AppInfo {
    version: String,
    os: &'static str,
    // This is the application's build target, not a hardware compatibility probe.
    architecture: &'static str,
}

#[tauri::command]
pub(crate) fn get_app_info<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> AppInfo {
    AppInfo {
        version: app.package_info().version.to_string(),
        os: std::env::consts::OS,
        architecture: std::env::consts::ARCH,
    }
}

#[cfg(test)]
mod tests {
    use super::get_app_info;
    use tauri::test::{mock_app, mock_builder, mock_context, noop_assets};

    #[test]
    fn command_uses_tauri_metadata_when_its_version_differs_from_cargo() {
        let mut context = mock_context(noop_assets());
        let configured_version = "9.8.7-metadata-test.1";
        assert_ne!(configured_version, env!("CARGO_PKG_VERSION"));
        context.package_info_mut().version = configured_version.parse().unwrap();
        let app = mock_builder().build(context).expect("test app must build");
        let info = get_app_info(app.handle().clone());

        assert_eq!(info.version, configured_version);
        assert_eq!(info.os, std::env::consts::OS);
        assert_eq!(info.architecture, std::env::consts::ARCH);
    }

    #[test]
    fn serialization_exposes_only_the_supported_string_fields() {
        let app = mock_app();
        let value = serde_json::to_value(get_app_info(app.handle().clone()))
            .expect("app information must serialize");
        let fields = value
            .as_object()
            .expect("app information must be an object");

        assert_eq!(fields.len(), 3);
        for key in ["version", "os", "architecture"] {
            let text = fields[key]
                .as_str()
                .expect("app information fields must be strings");
            assert!(!text.is_empty(), "{key} must not be empty");
        }
    }
}
