use std::io;
use std::io::ErrorKind;
use std::process::Command as StdCommand; // 为了区分
use tauri::async_runtime::spawn_blocking;


// 为Tauri命令或异步函数包装adb操作
pub async fn adb_pull_cmd(adb_path: &str, device_id: Option<&str>, remote_path: &str, local_path: &str) -> Result<String, String> {
    let adb_path = adb_path.to_string();
    let remote_path = remote_path.to_string();
    let local_path = local_path.to_string();
    let device_args = device_id.map(|id| vec!["-s".to_string(), id.to_string()]).unwrap_or_default();

    spawn_blocking(move || {
        let mut cmd = StdCommand::new(&adb_path);
        if !device_args.is_empty() {
            cmd.args(&device_args);
        }
        let output = cmd.args(&["pull", &remote_path, &local_path])
            .output()
            .map_err(|e| format!("执行 adb pull 失败: {}", e))?;

        if output.status.success() {
            Ok(format!("文件拉取成功: {}", local_path))
        } else {
            Err(format!("文件拉取失败 ({}): {}", local_path, String::from_utf8_lossy(&output.stderr)))
        }
    }).await.map_err(|e| format!("adb_pull spawn_blocking error: {}", e))?
}

pub async fn adb_push_cmd(adb_path: &str, device_id: Option<&str>, local_path: &str, remote_path: &str) -> Result<String, String> {
    let adb_path = adb_path.to_string();
    let local_path = local_path.to_string();
    let remote_path = remote_path.to_string() + "/license.lic";
    let device_args = device_id.map(|id| vec!["-s".to_string(), id.to_string()]).unwrap_or_default();


    spawn_blocking(move || {
        let mut cmd = StdCommand::new(&adb_path);
        if !device_args.is_empty() {
            cmd.args(&device_args);
        }
        let output = cmd.args(&["push", &local_path, &remote_path])
            .output()
            .map_err(|e| format!("执行 adb push 失败: {}", e))?;

        if output.status.success() {
            Ok(format!("文件推送成功: {}", remote_path))
        } else {
            Err(format!("文件推送失败 ({}): {}", remote_path, String::from_utf8_lossy(&output.stderr)))
        }
    }).await.map_err(|e| format!("adb_push spawn_blocking error: {}",e))?
}

pub async fn adb_devices_cmd(adb_path: &str) -> Result<Vec<String>, String> {
    let adb_path = adb_path.to_string();
    spawn_blocking(move || {
        let output = StdCommand::new(&adb_path).arg("devices").output()
            .map_err(|e| format!("执行 adb devices 失败: {}", e))?;

        if output.status.success() {
            let devices_str = String::from_utf8_lossy(&output.stdout);
            let device_list: Vec<String> = devices_str
                .lines()
                .filter_map(|line| {
                    let trimmed_line = line.trim();
                    if trimmed_line.ends_with("device") { // 更可靠的过滤方式
                        trimmed_line.split_whitespace().next().map(String::from)
                    } else {
                        None
                    }
                })
                .collect();
            Ok(device_list)
        } else {
            Err(format!("获取设备列表失败: {}", String::from_utf8_lossy(&output.stderr)))
        }
    }).await.map_err(|e| format!("adb_devices spawn_blocking error: {}", e))?
}


pub async fn adb_kill_cmd(adb_path: &str) -> Result<String, String> {
    let adb_path = adb_path.to_string();
    spawn_blocking(move || {
        StdCommand::new(&adb_path).arg("kill-server").output()
            .map_err(|e| format!("执行 adb kill-server 失败: {}", e))?;
        Ok("ADB server killed.".to_string())
    }).await.map_err(|e| format!("adb_kill spawn_blocking error: {}", e))?
}