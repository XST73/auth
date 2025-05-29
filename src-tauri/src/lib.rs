mod adb_utils;
mod en_de_crypt;
mod get_device_code;

use tauri::path::BaseDirectory;
use tauri::{AppHandle, Emitter, Manager, State, Window, Wry}; // Wry 是默认运行时
use tauri_plugin_dialog::DialogExt; // 引入 DialogExt trait 以便在 AppHandle 上使用 .dialog()
use tauri_plugin_fs::FsExt; // 引入 FsExt trait
use tauri_plugin_shell::ShellExt; // 引入 ShellExt trait

// 标准库和第三方库
use std::fs;
use std::io; // 保持 io 的使用
use std::path::{Path, PathBuf};
use std::str as StdStr; // 避免与 tauri::str 冲突
use std::sync::Mutex;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::en_de_crypt::{decrypt, encrypt};

const AUTH_FILE: &str = "license.lic";
const DEVICE_CODE_FILE: &str = "device_code.bin";

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AuthorizationData {
    pub device_code: String,
    pub issued_at: DateTime<Utc>,
    pub serial_number: String,
    pub checksum: String,
}

// 应用状态
pub struct AppState {
    adb_path: Mutex<String>,
    // log_messages: Mutex<Vec<String>>, // 如果需要后端存储日志
    // app_handle: Option<AppHandle<Wry>>, // 如果命令中需要 AppHandle 但不想通过参数传递
}

impl AppState {
    fn new(app_handle: &AppHandle<Wry>) -> Self {
        let adb_executable_name = if cfg!(windows) { "adb.exe" } else { "adb" };

        // 使用 Path 插件的 resolve 方法
        let adb_resource_path = app_handle
            .path()
            .resolve(
                // 注意：确保 "platform-tools" 文件夹在 Tauri.toml 中被配置为 resource
                // 并且该文件夹位于 src-tauri 目录下，或相对于 src-tauri 的正确路径
                format!("platform-tools/{}", adb_executable_name),
                BaseDirectory::Resource,
            )
            .expect("Failed to resolve ADB resource path. Ensure 'platform-tools' is in resources and path is correct in Tauri.toml");

        Self {
            adb_path: Mutex::new(adb_resource_path.to_string_lossy().into_owned()),
            // log_messages: Mutex::new(Vec::new()),
            // app_handle: Some(app_handle.clone()),
        }
    }
}

// 日志辅助函数 (可以移到 AppState impl 中或单独模块)
fn log_to_frontend<S: Into<String> + serde::Serialize>(
    window: &Window<Wry>,
    level: &str,
    message: S,
) {
    let log_entry = format!("[{}] {}", level.to_uppercase(), message.into());
    println!("{}", log_entry); // 后端控制台日志
    if let Err(e) = window.emit("log_message", log_entry) {
        eprintln!("Failed to emit log event: {}", e);
    }
}

// 生成校验和 (内部函数)
fn generate_checksum_internal(
    device_code: &str,
    serial_number: &str,
    issued_at: &DateTime<Utc>,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("{}{}{}", device_code, serial_number, issued_at).as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{:02x}", b))
        .collect::<String>()
}

// --- Tauri 命令 ---

#[tauri::command]
async fn list_adb_devices(
    window: Window<Wry>,
    app_state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    log_to_frontend(&window, "info", "请求刷新 ADB 设备列表...");
    let adb_p = app_state.adb_path.lock().unwrap().clone();
    match adb_utils::adb_devices_cmd(&adb_p).await {
        Ok(devices) => {
            log_to_frontend(&window, "info", format!("发现设备: {:?}", devices));
            Ok(devices)
        }
        Err(e) => {
            let err_msg = format!("获取设备列表失败: {}", e);
            log_to_frontend(&window, "error", err_msg.clone());
            Err(err_msg)
        }
    }
}

#[tauri::command]
async fn generate_windows_device_code(window: Window<Wry>) -> Result<String, String> {
    log_to_frontend(&window, "info", "请求生成 Windows 设备码...");
    match get_device_code::generate_device_code() {
        Some(code) => {
            // 注意：原函数会写入 device_code.bin。这里仅返回，由前端决定是否保存。
            // fs::write("device_code.bin", code.as_bytes()).unwrap_or_default();
            let msg = format!("Windows 设备码已生成: {}", code);
            log_to_frontend(&window, "info", msg);
            Ok(code)
        }
        None => {
            let err_msg = "生成 Windows 设备码失败 (可能无法获取主板序列号)".to_string();
            log_to_frontend(&window, "error", err_msg.clone());
            Err(err_msg)
        }
    }
}

#[tauri::command]
async fn generate_auth_file_cmd(
    window: Window<Wry>,
    device_code: String,
    target_path_str: String, // 用户选择的 license.lic 保存目录
) -> Result<String, String> {
    log_to_frontend(
        &window,
        "info",
        format!(
            "请求为设备 {} 在路径 {} 生成授权文件",
            device_code, target_path_str
        ),
    );

    let key_data = "6a1c6109e26cad37f6295bd3f3c270447f9272c4318237685b6c411d3a34359e";
    let key = hex::decode(key_data).map_err(|e| {
        log_to_frontend(&window, "error", format!("密钥解码失败: {}", e));
        e.to_string()
    })?;
    let path = PathBuf::from(target_path_str);

    if !path.is_dir() {
        let err_msg = format!("提供的路径不是一个有效的目录: {}", path.display());
        log_to_frontend(&window, "error", err_msg.clone());
        return Err(err_msg);
    }

    let serial_number = Uuid::new_v4().to_string();
    let issued_at = Utc::now();

    let auth_data = AuthorizationData {
        device_code: device_code.clone(),
        issued_at,
        serial_number: serial_number.clone(),
        checksum: generate_checksum_internal(&device_code, &serial_number, &issued_at),
    };

    let json_data = serde_json::to_string(&auth_data).map_err(|e| e.to_string())?;
    let (encrypted_data, nonce) = encrypt(&json_data, &key);

    let auth_file_full_path = path.join(AUTH_FILE);
    fs::write(
        &auth_file_full_path,
        format!("{}:{}", encrypted_data, nonce),
    )
    .map_err(|e| {
        log_to_frontend(&window, "error", format!("写入授权文件失败: {}", e));
        e.to_string()
    })?;

    let success_msg = format!(
        "授权文件已为设备 {} 生成在 {}",
        device_code,
        auth_file_full_path.display()
    );
    log_to_frontend(&window, "info", success_msg.clone());
    Ok(success_msg)
}

#[tauri::command]
async fn check_authorization_cmd(
    window: Window<Wry>,
    device_code_file_path_str: String, // device_code.bin 的完整路径
    auth_file_path_str: String,        // license.lic 的完整路径
) -> Result<AuthorizationData, String> {
    log_to_frontend(
        &window,
        "info",
        format!(
            "请求验证授权文件: {} (设备码文件: {})",
            auth_file_path_str, device_code_file_path_str
        ),
    );

    let key_data = "6a1c6109e26cad37f6295bd3f3c270447f9272c4318237685b6c411d3a34359e";
    let key = hex::decode(key_data).map_err(|e| e.to_string())?;

    let auth_file_path = PathBuf::from(auth_file_path_str);
    let device_code_file_path = PathBuf::from(device_code_file_path_str);

    if !auth_file_path.exists() {
        let err_msg = "未找到授权文件".to_string();
        log_to_frontend(&window, "error", err_msg.clone());
        return Err(err_msg);
    }
    if !device_code_file_path.exists() {
        let err_msg = format!("未找到设备码文件: {}", device_code_file_path.display());
        log_to_frontend(&window, "error", err_msg.clone());
        return Err(err_msg);
    }

    let device_code_from_file = fs::read_to_string(&device_code_file_path)
        .map_err(|e| format!("读取设备码文件失败: {}", e))?;
    let device_code_from_file = device_code_from_file.trim();

    let encrypted_content =
        fs::read_to_string(&auth_file_path).map_err(|e| format!("读取授权文件失败: {}", e))?;
    let parts: Vec<&str> = encrypted_content.split(':').collect();
    if parts.len() != 2 {
        let err_msg = "授权文件格式错误 (无法分割加密数据和nonce)".to_string();
        log_to_frontend(&window, "error", err_msg.clone());
        return Err(err_msg);
    }
    let encrypted_data_str = parts[0];
    let nonce_str = parts[1];

    let json_data = decrypt(encrypted_data_str, nonce_str, &key);
    let auth_data: AuthorizationData = serde_json::from_str(&json_data).map_err(|e| {
        let err_msg = format!("授权文件内容解析错误: {}", e);
        log_to_frontend(&window, "error", err_msg.clone());
        err_msg
    })?;

    if auth_data.device_code != device_code_from_file {
        let err_msg = format!(
            "设备码不匹配 (授权文件: {}, 设备文件: {})",
            auth_data.device_code, device_code_from_file
        );
        log_to_frontend(&window, "error", err_msg.clone());
        return Err(err_msg);
    }

    let expected_checksum = generate_checksum_internal(
        &auth_data.device_code,
        &auth_data.serial_number,
        &auth_data.issued_at,
    );
    if auth_data.checksum != expected_checksum {
        let err_msg = "授权文件校验和不匹配，可能被篡改".to_string();
        log_to_frontend(&window, "error", err_msg.clone());
        return Err(err_msg);
    }

    log_to_frontend(&window, "info", "授权验证通过，授权文件正常".to_string());
    Ok(auth_data)
}

async fn pull_and_auth_internal(
    window: &Window<Wry>,
    app_state: &AppState,
    adb_path: &str,
    device_id: &str,
    remote_path_base: &str,
    temp_dir: &Path, // 用于存放拉取和生成的临时文件
) -> Result<String, String> {
    log_to_frontend(
        window,
        "info",
        format!(
            "尝试从设备 {} 的路径 {} 拉取设备码",
            device_id, remote_path_base
        ),
    );

    let remote_device_code_file_on_device = format!("{}{}", remote_path_base, DEVICE_CODE_FILE);
    let local_temp_device_code_file = temp_dir.join(format!("{}_{}", device_id, DEVICE_CODE_FILE)); //确保每个设备文件名唯一
    let local_temp_auth_file = temp_dir.join(format!("{}_{}", device_id, AUTH_FILE));

    adb_utils::adb_pull_cmd(
        adb_path,
        Some(device_id),
        &remote_device_code_file_on_device,
        local_temp_device_code_file.to_str().unwrap(),
    )
    .await
    .map_err(|e| format!("设备码拉取失败 ({}): {}", device_id, e))?;
    log_to_frontend(
        window,
        "info",
        format!(
            "设备码 {} 已拉取成功到 {}",
            device_id,
            local_temp_device_code_file.display()
        ),
    );

    let device_code_content = fs::read_to_string(&local_temp_device_code_file).map_err(|e| {
        format!(
            "读取临时设备码文件 {} 失败: {}",
            local_temp_device_code_file.display(),
            e
        )
    })?;
    let device_code_trimmed = device_code_content.trim().to_string();
    log_to_frontend(
        window,
        "info",
        format!(
            "准备为设备 {} (设备码: {}) 授权",
            device_id, device_code_trimmed
        ),
    );

    let key_data = "6a1c6109e26cad37f6295bd3f3c270447f9272c4318237685b6c411d3a34359e";
    let key = hex::decode(key_data).map_err(|e| e.to_string())?;

    let serial_number = Uuid::new_v4().to_string();
    let issued_at = Utc::now();
    let auth_data_struct = AuthorizationData {
        device_code: device_code_trimmed.clone(),
        issued_at,
        serial_number: serial_number.clone(),
        checksum: generate_checksum_internal(&device_code_trimmed, &serial_number, &issued_at),
    };
    let json_data = serde_json::to_string(&auth_data_struct).map_err(|e| e.to_string())?;
    let (encrypted_data, nonce) = encrypt(&json_data, &key);
    fs::write(
        &local_temp_auth_file,
        format!("{}:{}", encrypted_data, nonce),
    )
    .map_err(|e| {
        format!(
            "写入临时授权文件 {} 失败: {}",
            local_temp_auth_file.display(),
            e
        )
    })?;
    log_to_frontend(
        window,
        "info",
        format!(
            "临时授权文件为设备 {} 已生成在 {}",
            device_id,
            local_temp_auth_file.display()
        ),
    );

    adb_utils::adb_push_cmd(
        adb_path,
        Some(device_id),
        local_temp_auth_file.to_str().unwrap(),
        remote_path_base,
    )
    .await
    .map_err(|e| format!("授权文件推送失败 ({}): {}", device_id, e))?;
    log_to_frontend(
        window,
        "info",
        format!("授权文件已推送到设备 {} 的 {}", device_id, remote_path_base),
    );

    let _ = fs::remove_file(&local_temp_device_code_file);
    let _ = fs::remove_file(&local_temp_auth_file);
    log_to_frontend(
        window,
        "info",
        format!("已清理设备 {} 的临时文件", device_id),
    );

    Ok(format!(
        "设备 {} @ {} 授权成功",
        device_id, remote_path_base
    ))
}

#[tauri::command]
async fn process_android_authorization(
    window: Window<Wry>,
    batch_mode: bool,
    app_state: State<'_, AppState>,
    app_handle: AppHandle<Wry>, // AppHandle 可以用来获取路径等
) -> Result<String, String> {
    let state_inner = app_state.inner();
    let adb_p_clone = state_inner.adb_path.lock().unwrap().clone();
    log_to_frontend(
        &window,
        "info",
        format!("开始处理 Android 授权, 批量模式: {}", batch_mode),
    );

    // 你的原始代码中的路径
    let dcp_android_data_paths = [
        "/storage/emulated/0/Android/data/alvr.client.stable/files/BUPT-VR_Client/",
        // 可以添加更多可能的路径
    ];
    let dcp_android_media_paths = [
        "/storage/emulated/0/Android/media/alvr.client.stable/files/",
        // 可以添加更多可能的路径
    ];

    let all_remote_paths: Vec<&str> = dcp_android_data_paths
        .iter()
        .cloned()
        .chain(dcp_android_media_paths.iter().cloned())
        .collect();

    let devices = adb_utils::adb_devices_cmd(&adb_p_clone)
        .await
        .map_err(|e| e.to_string())?;
    if devices.is_empty() {
        let msg = "未检测到设备，请检查连接".to_string();
        log_to_frontend(&window, "error", msg.clone());
        return Err(msg);
    }

    let mut overall_results = Vec::new();

    let temp_dir_path = app_handle.path().temp_dir().unwrap();
    if !temp_dir_path.exists() {
        fs::create_dir_all(&temp_dir_path).map_err(|e| format!("创建临时目录失败: {}", e))?;
    }

    for device_id in devices {
        log_to_frontend(&window, "info", format!("开始处理设备: {}", device_id));
        let mut device_authorized_successfully = false;
        for remote_base in &all_remote_paths {
            log_to_frontend(
                &window,
                "info",
                format!("设备 {}, 尝试路径: {}", device_id, remote_base),
            );
            match pull_and_auth_internal(
                &window,
                &state_inner,
                &adb_p_clone,
                &device_id,
                remote_base, // 当前尝试的路径
                &temp_dir_path,
            )
            .await
            {
                Ok(msg) => {
                    overall_results.push(msg);
                    device_authorized_successfully = true;
                    break; // 当前设备此路径成功，处理下一个设备或结束
                }
                Err(e) => {
                    log_to_frontend(
                        &window,
                        "warn",
                        format!(
                            "设备 {} 在路径 {} 授权失败: {}. 尝试下一个可用路径...",
                            device_id, remote_base, e
                        ),
                    );
                    // 不需要立即将错误添加到 overall_results，除非所有路径都失败
                }
            }
        }
        if !device_authorized_successfully {
            let err_msg = format!("设备 {} 在所有已知路径均授权失败。", device_id);
            log_to_frontend(&window, "error", err_msg.clone());
            overall_results.push(err_msg);
        }

        if !batch_mode && !overall_results.is_empty() {
            // 如果非批量模式，且已有结果(成功或失败)，则停止
            break;
        }
    }

    if let Err(e) = adb_utils::adb_kill_cmd(&adb_p_clone).await {
        log_to_frontend(&window, "error", format!("关闭 ADB 服务失败: {}", e));
    } else {
        log_to_frontend(&window, "info", "ADB服务已关闭".to_string());
    }

    Ok(overall_results.join("\n"))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_fs::init())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            app.manage(AppState::new(&app.handle()));
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_adb_devices,
            generate_windows_device_code,
            // authorize_windows, // 这是一个复合命令，你可能想分解它或按原样保留
            generate_auth_file_cmd,
            check_authorization_cmd,
            process_android_authorization
        ])
        .run(tauri::generate_context!()) // generate_context! 会读取 Tauri.toml
        .expect("error while running tauri application");
}
