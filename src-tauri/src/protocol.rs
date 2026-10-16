use std::sync::Mutex;
use tauri::Manager;
#[cfg(any(windows, target_os = "linux"))]
use tauri_plugin_deep_link::DeepLinkExt;

#[derive(Default)]
pub struct ProtocolRuntime(Mutex<Option<&'static str>>);

#[cfg(any(windows, target_os = "linux", test))]
fn register_and_verify(
    register: impl FnOnce() -> Result<(), ()>,
    verify: impl FnOnce() -> Result<bool, ()>,
) -> Result<(), &'static str> {
    register().map_err(|_| "protocolRegister")?;
    match verify() {
        Ok(true) => Ok(()),
        _ => Err("protocolVerify"),
    }
}

pub fn register(app: &tauri::AppHandle) -> Result<(), &'static str> {
    #[cfg(any(windows, target_os = "linux"))]
    {
        register_and_verify(
            || app.deep_link().register("skillshub").map_err(|_| ()),
            || app.deep_link().is_registered("skillshub").map_err(|_| ()),
        )
    }
    #[cfg(not(any(windows, target_os = "linux")))]
    {
        let _ = app;
        Err("protocolInstalledOnly")
    }
}

#[cfg(all(windows, not(debug_assertions)))]
pub fn remember_startup_error(app: &tauri::AppHandle, error: &'static str) {
    if let Ok(mut pending) = app.state::<ProtocolRuntime>().0.lock() {
        *pending = Some(error);
    }
}

#[tauri::command]
pub fn take_protocol_registration_error(app: tauri::AppHandle) -> Option<String> {
    app.state::<ProtocolRuntime>()
        .0
        .lock()
        .ok()?
        .take()
        .map(str::to_owned)
}

#[tauri::command]
pub fn register_skillshub_protocol(app: tauri::AppHandle) -> Result<(), String> {
    register(&app).map_err(str::to_owned)?;
    // A successful explicit repair supersedes an earlier startup failure.
    let _ = take_protocol_registration_error(app);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_successful_write_without_effective_handler() {
        assert_eq!(
            register_and_verify(|| Ok(()), || Ok(false)),
            Err("protocolVerify")
        );
        assert_eq!(
            register_and_verify(|| Ok(()), || Err(())),
            Err("protocolVerify")
        );
    }

    #[test]
    fn checks_only_after_write_and_accepts_verified_handler() {
        assert_eq!(
            register_and_verify(|| Err(()), || panic!("write failed")),
            Err("protocolRegister")
        );
        assert_eq!(register_and_verify(|| Ok(()), || Ok(true)), Ok(()));
    }
}
