//! Offline cache maintenance through the transactional Store. Never edits skill files.
fn main() {
    let result = (|| -> Result<(), String> {
        let root = std::env::args_os()
            .nth(1)
            .ok_or("Expected an explicit .arsenal directory")?;
        let root = std::path::absolute(root).map_err(|_| "Invalid directory")?;
        if root.file_name().and_then(|n| n.to_str()) != Some(".arsenal")
            || !root.join("store/manifest.json").is_file()
        {
            return Err("Expected an existing .arsenal store".into());
        }
        let store = skills_manage_lib::storage::store::Store::open(&root)?;
        let bytes = || {
            store.read(|d| {
                Ok(d.table("repository_mirror_snapshots")
                    .values()
                    .filter_map(|row| row["payload"].as_str())
                    .map(str::len)
                    .sum::<usize>())
            })
        };
        let before = bytes()?;
        let args = std::env::args().collect::<Vec<_>>();
        if let Some(index) = args.iter().position(|arg| arg == "--retain-file") {
            let file = args.get(index + 1).ok_or("Expected retention file")?;
            let retained: Vec<String> = serde_json::from_slice(
                &std::fs::read(file).map_err(|_| "Cannot read retention file")?,
            )
            .map_err(|_| "Invalid retention file")?;
            tauri::async_runtime::block_on(
                skills_manage_lib::commands::repository_mirror::reconcile_snapshot_cache(
                    &store, &retained,
                ),
            )?;
        }
        if std::env::args().any(|arg| arg == "--compact") {
            let changed = tauri::async_runtime::block_on(
                skills_manage_lib::commands::repository_mirror::compact_snapshot_cache(&store),
            )?;
            println!(
                "Compacted snapshots: {changed}; bytes before: {before}; bytes after: {}",
                bytes()?
            );
        } else {
            println!("Snapshot payload bytes: {before}");
        }
        Ok(())
    })();
    if result.is_err() {
        // Avoid paths and arbitrary store content in console diagnostics.
        eprintln!("Maintenance failed; ensure Arsenal is closed and the store is valid. No direct file edits were attempted.");
        std::process::exit(1);
    }
}
