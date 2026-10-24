use serde_json::Value;
use std::path::Path;

const MAX_BYTES: u64 = 180_000_000;

pub fn load(dir: &Path) -> Result<Option<Value>, String> {
    let file = dir.join("collection.json");
    if !file.exists() {
        return Ok(None);
    }
    if std::fs::metadata(&file).map_err(|e| e.to_string())?.len() > MAX_BYTES {
        return Err("收藏文件过大，请先备份后整理。".into());
    }
    let bytes = std::fs::read(file).map_err(|e| e.to_string())?;
    serde_json::from_slice(&bytes).map(Some).map_err(|e| {
        format!(
            "收藏读取失败，原文件已保留。可从数据目录中的 collection.previous.json 恢复上一版：{e}"
        )
    })
}

pub fn save(dir: &Path, data: &Value) -> Result<(), String> {
    if data.get("version").and_then(Value::as_u64) != Some(1)
        || !data.get("photos").is_some_and(Value::is_array)
        || !data.get("notes").is_some_and(Value::is_array)
    {
        return Err("收藏数据格式异常，已停止保存。".into());
    }
    let bytes = serde_json::to_vec(data).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("收藏较大，请先导出备份并移走部分照片。".into());
    }
    std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let file = dir.join("collection.json");
    let previous = if file.exists() {
        let old = std::fs::read(&file).map_err(|e| e.to_string())?;
        serde_json::from_slice::<Value>(&old)
            .map_err(|_| "原收藏文件异常，已停止保存并保留原文件。")?;
        Some(old)
    } else {
        None
    };
    let tmp = dir.join("collection.tmp");
    std::fs::write(&tmp, bytes).map_err(|e| e.to_string())?;
    if let Some(previous) = previous {
        let backup_tmp = dir.join("collection.previous.tmp");
        std::fs::write(&backup_tmp, previous).map_err(|e| e.to_string())?;
        std::fs::rename(backup_tmp, dir.join("collection.previous.json"))
            .map_err(|e| e.to_string())?;
    }
    std::fs::rename(tmp, file).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn sandbox() -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "darkroom-storage-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }
    #[test]
    fn saving_keeps_previous_collection_for_recovery() {
        let dir = sandbox();
        let first = serde_json::json!({"version":1,"photos":[],"notes":[{"text":"first"}]});
        let second = serde_json::json!({"version":1,"photos":[],"notes":[{"text":"second"}]});
        save(&dir, &first).unwrap();
        save(&dir, &second).unwrap();
        assert_eq!(load(&dir).unwrap(), Some(second));
        assert_eq!(
            serde_json::from_slice::<Value>(
                &std::fs::read(dir.join("collection.previous.json")).unwrap()
            )
            .unwrap(),
            first
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
    #[test]
    fn corrupt_collection_is_never_overwritten() {
        let dir = sandbox();
        std::fs::write(dir.join("collection.json"), b"broken json").unwrap();
        let data = serde_json::json!({"version":1,"photos":[],"notes":[]});
        assert!(load(&dir).is_err());
        assert!(save(&dir, &data).is_err());
        assert_eq!(
            std::fs::read(dir.join("collection.json")).unwrap(),
            b"broken json"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
