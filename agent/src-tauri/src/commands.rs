use serde_json::json;

#[tauri::command]
pub fn load_config() -> Result<serde_json::Value, String> {
    let path = crate::db::db_path();
    let seed = crate::db::seed_override();
    let file =
        crate::db::load_config(&path, seed.as_deref()).map_err(|error| format!("{error:#}"))?;
    let value = serde_json::to_value(&file).map_err(|error| error.to_string())?;
    let revision = crate::db::revision(&path).unwrap_or(0);
    Ok(json!({
        "path": path.display().to_string(),
        "revision": revision,
        "value": value,
    }))
}

#[tauri::command]
pub fn save_config(value: serde_json::Value) -> Result<serde_json::Value, String> {
    let path = crate::db::db_path();
    let file: crate::config::ConfigFile =
        serde_json::from_value(value).map_err(|error| format!("invalid state: {error}"))?;
    let revision = crate::db::save_config(&path, &file).map_err(|error| format!("{error:#}"))?;
    Ok(json!({
        "path": path.display().to_string(),
        "revision": revision,
    }))
}

#[tauri::command]
pub fn reset_config() -> Result<serde_json::Value, String> {
    let path = crate::db::db_path();
    let revision = crate::db::reset(&path).map_err(|error| format!("{error:#}"))?;
    Ok(json!({
        "path": path.display().to_string(),
        "revision": revision,
    }))
}

#[tauri::command]
pub fn export_config() -> Result<String, String> {
    let path = crate::db::db_path();
    let destination = path.with_file_name("export.yaml");
    crate::db::export_yaml(&path, &destination).map_err(|error| format!("{error:#}"))?;
    Ok(destination.display().to_string())
}

#[tauri::command]
pub fn current_app() -> serde_json::Value {
    match crate::context::last_foreign() {
        Some(focus) => json!({ "exe": focus.exe, "title": focus.title }),
        None => json!({ "exe": "", "title": "" }),
    }
}

#[tauri::command]
pub fn reveal_config() -> Result<(), String> {
    let path = crate::db::db_path();
    if let Some(parent) = path.parent() {
        std::process::Command::new("explorer")
            .arg(parent)
            .spawn()
            .map_err(|error| error.to_string())?;
    }
    Ok(())
}

/// Draw a specific legend. Used by the editor preview and by tests.
#[tauri::command]
pub fn show_osd(
    app: tauri::AppHandle,
    payload: serde_json::Value,
    sticky: Option<bool>,
    monitor: Option<String>,
) -> Result<(), String> {
    let sticky = sticky.unwrap_or(false);
    let duration = if sticky { 0 } else { 2500 };
    crate::osd::show(
        &app,
        payload,
        duration,
        sticky,
        monitor.as_deref().unwrap_or("focused"),
    );
    Ok(())
}
