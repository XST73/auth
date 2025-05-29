mod adb_utils;
mod en_de_crypt;
mod get_device_code;

use tauri::path::{BaseDirectory, PathResolver};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, Window, Wry};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_fs::FsExt;
use tauri_plugin_shell::ShellExt;

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::{env, fs};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::en_de_crypt::{decrypt, encrypt};

use std::sync::atomic::{AtomicBool, Ordering};

// 全局静态变量，用于标记是否正在退出
static IS_EXITING: AtomicBool = AtomicBool::new(false);

const AUTH_FILE_NAME: &str = "license.lic"; // Renamed for clarity
const DEVICE_CODE_FILE_NAME: &str = "device_code.bin"; // Renamed for clarity

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct AuthorizationData {
    pub device_code: String,
    pub issued_at: DateTime<Utc>,
    pub serial_number: String,
    pub checksum: String,
}

// New struct to return combined authorization and verification result
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct WindowsAuthResult {
    pub authorization_message: String,
    pub verification_status: String,
    pub verification_details: Option<AuthorizationData>, // None if verification fails
}

pub struct AppState {
    adb_path: Mutex<String>,
}

impl AppState {
    fn new(app_handle: &AppHandle<Wry>) -> Self {
        let adb_executable_name = if cfg!(windows) { "adb.exe" } else { "adb" };
        let adb_resource_path = app_handle
            .path()
            .resolve(
                format!("platform-tools/{}", adb_executable_name),
                BaseDirectory::Resource,
            )
            .expect("Failed to resolve ADB resource path.");
        Self {
            adb_path: Mutex::new(adb_resource_path.to_string_lossy().into_owned()),
        }
    }
}

fn log_to_frontend<S: Into<String> + serde::Serialize>(
    window: &Window<Wry>,
    level: &str,
    message: S,
) {
    let log_entry = format!("[{}] {}", level.to_uppercase(), message.into());
    println!("{}", log_entry);
    if let Err(e) = window.emit("log_message", log_entry) {
        eprintln!("Failed to emit log event: {}", e);
    }
}

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

// Renamed from generate_auth_file_cmd to reflect its new role and parameters
// This function will now be called internally by authorize_windows_application
async fn generate_auth_file_for_app(
    window: &Window<Wry>, // Passed as reference
    device_code: String,
    target_app_path: &Path, // Use Path directly
) -> Result<PathBuf, String> {
    // Returns path to the generated .lic file
    log_to_frontend(
        window,
        "info",
        format!(
            "为设备 {} 在应用路径 {} 生成授权文件",
            device_code,
            target_app_path.display()
        ),
    );

    let key_data = "6a1c6109e26cad37f6295bd3f3c270447f9272c4318237685b6c411d3a34359e";
    let key = hex::decode(key_data).map_err(|e| {
        log_to_frontend(window, "error", format!("密钥解码失败: {}", e));
        e.to_string()
    })?;

    if !target_app_path.is_dir() {
        let err_msg = format!(
            "提供的应用路径不是一个有效的目录: {}",
            target_app_path.display()
        );
        log_to_frontend(window, "error", err_msg.clone());
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

    let auth_file_full_path = target_app_path.join(AUTH_FILE_NAME);
    fs::write(
        &auth_file_full_path,
        format!("{}:{}", encrypted_data, nonce),
    )
    .map_err(|e| {
        let err_msg = format!(
            "写入授权文件失败 ({}): {}",
            auth_file_full_path.display(),
            e
        );
        log_to_frontend(window, "error", err_msg.clone());
        err_msg
    })?;

    Ok(auth_file_full_path)
}

// Modified to take app_path and derive file paths from it
// This will be called internally by authorize_windows_application
async fn check_authorization_for_app(
    window: &Window<Wry>, // Passed as reference
    app_path: &Path,
) -> Result<AuthorizationData, String> {
    let auth_file_path = app_path.join(AUTH_FILE_NAME);
    let device_code_file_path = app_path.join(DEVICE_CODE_FILE_NAME);

    log_to_frontend(
        window,
        "info",
        format!(
            "请求验证应用路径 {} 下的授权 (授权文件: {}, 设备码文件: {})",
            app_path.display(),
            auth_file_path.display(),
            device_code_file_path.display()
        ),
    );

    let key_data = "6a1c6109e26cad37f6295bd3f3c270447f9272c4318237685b6c411d3a34359e";
    let key = hex::decode(key_data).map_err(|e| e.to_string())?;

    if !auth_file_path.exists() {
        let err_msg = format!("未找到授权文件: {}", auth_file_path.display());
        log_to_frontend(window, "error", err_msg.clone());
        return Err(err_msg);
    }
    if !device_code_file_path.exists() {
        let err_msg = format!("未找到设备码文件: {}", device_code_file_path.display());
        log_to_frontend(window, "error", err_msg.clone());
        return Err(err_msg);
    }

    let device_code_from_file = fs::read_to_string(&device_code_file_path).map_err(|e| {
        format!(
            "读取设备码文件 {} 失败: {}",
            device_code_file_path.display(),
            e
        )
    })?;
    let device_code_from_file_trimmed = device_code_from_file.trim();

    let encrypted_content = fs::read_to_string(&auth_file_path)
        .map_err(|e| format!("读取授权文件 {} 失败: {}", auth_file_path.display(), e))?;
    let parts: Vec<&str> = encrypted_content.split(':').collect();
    if parts.len() != 2 {
        let err_msg = "授权文件格式错误 (无法分割加密数据和nonce)".to_string();
        log_to_frontend(window, "error", err_msg.clone());
        return Err(err_msg);
    }
    let encrypted_data_str = parts[0];
    let nonce_str = parts[1];

    let json_data = decrypt(encrypted_data_str, nonce_str, &key);
    let auth_data: AuthorizationData = serde_json::from_str(&json_data).map_err(|e| {
        let err_msg = format!("授权文件内容解析错误: {}", e);
        log_to_frontend(window, "error", err_msg.clone());
        err_msg
    })?;

    if auth_data.device_code != device_code_from_file_trimmed {
        let err_msg = format!(
            "设备码不匹配 (授权文件: {}, 设备文件: {})",
            auth_data.device_code, device_code_from_file_trimmed
        );
        log_to_frontend(window, "error", err_msg.clone());
        return Err(err_msg);
    }

    let expected_checksum = generate_checksum_internal(
        &auth_data.device_code,
        &auth_data.serial_number,
        &auth_data.issued_at,
    );
    if auth_data.checksum != expected_checksum {
        let err_msg = "授权文件校验和不匹配，可能被篡改".to_string();
        log_to_frontend(window, "error", err_msg.clone());
        return Err(err_msg);
    }

    Ok(auth_data)
}

// New command for the improved Windows authorization flow
#[tauri::command]
async fn authorize_windows_application(
    window: Window<Wry>,
    _app_handle: AppHandle<Wry>, // Added AppHandle
    application_path_str: String,
) -> Result<WindowsAuthResult, String> {
    log_to_frontend(
        &window,
        "info",
        format!(
            "开始 Windows 应用授权流程，目标路径: {}",
            application_path_str
        ),
    );

    let application_path = PathBuf::from(&application_path_str);
    if !application_path.is_dir() {
        let err_msg = format!(
            "提供的应用路径不是一个有效的目录: {}",
            application_path.display()
        );
        log_to_frontend(&window, "error", err_msg.clone());
        return Err(err_msg);
    }
    // Ensure the path is allowed by FS scope if it's not a subdirectory of a base directory
    // This might be needed if you use restricted FS scopes. For simplicity, we assume broad access or correct setup.
    // You might need to add `application_path_str` to `fs.scope` in `tauri.conf.json` or use `dialog.open`
    // which automatically grants temporary access. Since the user selects it, temporary access is granted.

    // 1. get device code from file
    let device_code = match fs::read_to_string(application_path.join(DEVICE_CODE_FILE_NAME)) {
        Ok(code) => {
            let trimmed_code = code.trim().to_string();
            if trimmed_code.is_empty() {
                let err_msg = "设备码文件内容为空".to_string();
                log_to_frontend(&window, "error", err_msg.clone());
                return Err(err_msg);
            }
            log_to_frontend(&window, "info", format!("读取到设备码: {}", trimmed_code));
            trimmed_code
        }
        Err(e) => {
            let err_msg = format!("读取设备码文件失败: {}", e);
            log_to_frontend(&window, "error", err_msg.clone());
            return Err(err_msg);
        }
    };

    // 2. Save device_code.bin to application_path
    let device_code_file_path = application_path.join(DEVICE_CODE_FILE_NAME);
    if let Err(e) = fs::write(&device_code_file_path, &device_code) {
        let err_msg = format!(
            "写入设备码文件 {} 失败: {}",
            device_code_file_path.display(),
            e
        );
        log_to_frontend(&window, "error", err_msg.clone());
        return Err(err_msg);
    }
    log_to_frontend(
        &window,
        "info",
        format!("设备码文件已保存到: {}", device_code_file_path.display()),
    );

    // 3. Generate license.lic in application_path
    let auth_file_full_path =
        match generate_auth_file_for_app(&window, device_code.clone(), &application_path).await {
            Ok(path) => {
                log_to_frontend(
                    &window,
                    "info",
                    format!("授权文件已生成在: {}", path.display()),
                );
                path
            }
            Err(e) => {
                log_to_frontend(&window, "error", format!("生成授权文件失败: {}", e));
                return Err(format!("生成授权文件失败: {}", e));
            }
        };

    let authorization_message = format!("授权文件已成功生成在 {}", auth_file_full_path.display());

    // 4. Automatically verify
    log_to_frontend(&window, "info", "开始自动校验生成的授权...");
    match check_authorization_for_app(&window, &application_path).await {
        Ok(auth_data) => {
            let success_msg = format!(
                "自动校验通过! 设备码: {}, 序列号: {}, 时间: {}",
                auth_data.device_code, auth_data.serial_number, auth_data.issued_at
            );
            log_to_frontend(&window, "info", success_msg.clone());
            Ok(WindowsAuthResult {
                authorization_message,
                verification_status: "校验通过".to_string(),
                verification_details: Some(auth_data),
            })
        }
        Err(e) => {
            log_to_frontend(&window, "error", format!("自动校验失败: {}", e));
            Ok(WindowsAuthResult {
                authorization_message, // Still report auth success
                verification_status: format!("校验失败: {}", e),
                verification_details: None,
            })
        }
    }
}

// src-tauri/src/lib.rs

// ... (之前的 AuthorizationData, WindowsAuthResult, AppState, log_to_frontend, generate_checksum_internal)

#[tauri::command]
async fn process_android_authorization(
    window: Window<Wry>,
    batch_mode: bool, // 重新接收 batch_mode
    app_state: State<'_, AppState>,
    app_handle: AppHandle<Wry>,
) -> Result<String, String> {
    let state_inner = app_state.inner();
    let adb_p_clone = state_inner.adb_path.lock().unwrap().clone();
    log_to_frontend(
        &window,
        "info",
        format!("开始处理 Android 授权, 批量模式: {}", batch_mode),
    );

    let dcp_android_data_paths = [
        "/storage/emulated/0/Android/data/alvr.client.stable/files/BUPT-VR_Client/",
        // Add more paths if needed
    ];
    let dcp_android_media_paths = [
        "/storage/emulated/0/Android/media/alvr.client.stable/files/",
        // Add more paths if needed
    ];

    let all_remote_paths: Vec<&str> = dcp_android_data_paths
        .iter()
        .cloned()
        .chain(dcp_android_media_paths.iter().cloned())
        .collect();

    let mut devices_to_process = adb_utils::adb_devices_cmd(&adb_p_clone)
        .await
        .map_err(|e| e.to_string())?;

    if devices_to_process.is_empty() {
        let msg = "未检测到设备，请连接设备后重试".to_string();
        log_to_frontend(&window, "error", msg.clone());
        return Err(msg);
    }

    if !batch_mode {
        // 如果不是批量模式，只取第一个设备
        if let Some(first_device) = devices_to_process.first().cloned() {
            devices_to_process = vec![first_device];
            log_to_frontend(
                &window,
                "info",
                format!("单设备模式：将处理设备 {}", devices_to_process.join(", ")),
            );
        } else {
            // Should not happen if devices_to_process was not empty, but defensive check
            return Err("无法确定要处理的单个设备".to_string());
        }
    } else {
        log_to_frontend(
            &window,
            "info",
            format!(
                "批量模式：将处理所有已连接设备: {}",
                devices_to_process.join(", ")
            ),
        );
    }

    let mut overall_results = Vec::new();
    let temp_dir_path = app_handle.path().temp_dir().unwrap();
    if !temp_dir_path.exists() {
        fs::create_dir_all(&temp_dir_path).map_err(|e| format!("创建临时目录失败: {}", e))?;
    }

    for device_id in devices_to_process {
        // Iterates over all (if batch) or just one (if not batch)
        log_to_frontend(&window, "info", format!("开始处理设备: {}", device_id));
        let mut device_authorized_successfully = false;
        let mut device_results = Vec::new(); // Results for the current device

        for remote_base in &all_remote_paths {
            log_to_frontend(
                &window,
                "info",
                format!("设备 {}, 尝试路径: {}", device_id, remote_base),
            );
            match pull_and_auth_internal(
                // This function was already processing one device at a time
                &window,
                &state_inner,
                &adb_p_clone,
                &device_id,
                remote_base,
                &temp_dir_path,
            )
            .await
            {
                Ok(msg) => {
                    device_results.push(msg);
                    device_authorized_successfully = true;
                    // In non-batch mode, or even in batch mode for a single device path,
                    // once a path works for a device, we are done with THIS device's paths.
                    break;
                }
                Err(e) => {
                    let path_err_msg = format!(
                        "设备 {} 在路径 {} 授权失败: {}.", // Keep this specific error log
                        device_id, remote_base, e
                    );
                    log_to_frontend(&window, "warn", path_err_msg);
                    // We don't add this to device_results immediately,
                    // only if all paths fail for this device.
                }
            }
        }

        if !device_authorized_successfully {
            let err_msg = format!("设备 {} 在所有已知路径均授权失败。", device_id);
            log_to_frontend(&window, "error", err_msg.clone());
            overall_results.push(err_msg); // Add the overarching failure for this device
        } else {
            overall_results.extend(device_results); // Add successes for this device
        }

        // If not in batch mode, we've processed the single device (or first), so we stop.
        // The loop `for device_id in devices_to_process` will only have one iteration if !batch_mode.
    }

    if overall_results.is_empty() {
        Ok("未处理任何设备或所有设备均处理失败。".to_string())
    } else {
        Ok(overall_results.join("\n"))
    }
}

// Helper for pull_and_auth_internal - unchanged for this request, but ensure it uses correct constants
async fn pull_and_auth_internal(
    window: &Window<Wry>,
    _app_state: &AppState, // Assuming AppState holds adb_path
    adb_path: &str,
    device_id: &str,
    remote_path_base: &str,
    temp_dir: &Path,
) -> Result<String, String> {
    log_to_frontend(
        window,
        "info",
        format!(
            "尝试从设备 {} 的路径 {} 拉取设备码",
            device_id, remote_path_base
        ),
    );

    let remote_device_code_file_on_device =
        format!("{}{}", remote_path_base, DEVICE_CODE_FILE_NAME); // Use constant
    let local_temp_device_code_file =
        temp_dir.join(format!("{}_{}", device_id, DEVICE_CODE_FILE_NAME)); // Use constant
    let local_temp_auth_file = temp_dir.join(format!("{}_{}", device_id, AUTH_FILE_NAME)); // Use constant

    adb_utils::adb_pull_cmd(
        adb_path,
        Some(device_id),
        &remote_device_code_file_on_device,
        local_temp_device_code_file
            .to_str()
            .ok_or("无效的本地临时设备码文件路径")?,
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

    // Ensure the remote path for push ends with a `/` if it's a directory,
    // or correctly names the target file if `adb_push_cmd` expects that.
    // The current `adb_push_cmd` appends "/license.lic", so `remote_path_base` should be the directory.
    let target_remote_dir = if remote_path_base.ends_with('/') {
        remote_path_base.to_string()
    } else {
        format!("{}/", remote_path_base)
    };

    adb_utils::adb_push_cmd(
        adb_path,
        Some(device_id),
        local_temp_auth_file
            .to_str()
            .ok_or("无效的本地临时授权文件路径")?,
        &target_remote_dir, // Pass the directory to adb_push_cmd
    )
    .await
    .map_err(|e| format!("授权文件推送失败 ({}): {}", device_id, e))?;
    log_to_frontend(
        window,
        "info",
        format!(
            "授权文件已推送到设备 {} 的 {}",
            device_id, target_remote_dir
        ),
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
async fn get_executable_dir(_app_handle: AppHandle) -> Result<String, String> {
    match env::current_exe() {
        Ok(exe_path) => {
            if let Some(exe_dir) = exe_path.parent() {
                Ok(exe_dir.to_string_lossy().into_owned())
            } else {
                Err("无法获取可执行文件的父目录".to_string())
            }
        }
        Err(e) => Err(format!("无法获取可执行文件路径: {}", e)),
    }
}

#[tauri::command]
async fn kill_adb_server_on_exit(
    app_handle: AppHandle<Wry>, // AppHandle 可以用来获取 State
) -> Result<String, String> {
    println!("[INFO] 应用退出前：尝试关闭 ADB 服务..."); // 使用 println! 因为此时窗口可能已关闭
    let app_state: State<'_, AppState> = app_handle.state(); // 获取 AppState
    let adb_p = app_state.adb_path.lock().unwrap().clone();
    match adb_utils::adb_kill_cmd(&adb_p).await {
        Ok(msg) => {
            println!("[INFO] ADB 服务关闭成功: {}", msg);
            Ok(msg)
        }
        Err(e) => {
            eprintln!("[ERROR] 关闭 ADB 服务失败: {}", e);
            Err(e)
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let builder = tauri::Builder::default()
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
            authorize_windows_application,
            process_android_authorization,
            get_executable_dir,
            kill_adb_server_on_exit
        ]);

    builder
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|app_handle, event| match event {
            RunEvent::ExitRequested { api, .. } => {
                // 尝试获取退出锁，如果成功（之前是false，现在设置为true），则执行清理
                if IS_EXITING
                    .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
                {
                    println!("[INFO] 应用退出请求 (ExitRequested)，首次进入，准备关闭 ADB 服务...");
                    let app_handle_clone = app_handle.clone();
                    api.prevent_exit(); // 阻止立即退出

                    tauri::async_runtime::spawn(async move {
                        println!("[INFO] 在异步任务中执行 kill_adb_server_on_exit...");
                        match kill_adb_server_on_exit(app_handle_clone.clone()).await {
                            Ok(_) => {
                                println!("[INFO] ADB 服务已成功关闭 (来自 ExitRequested 事件)。")
                            }
                            Err(e) => eprintln!("[ERROR] 应用退出时关闭 ADB 服务失败: {}", e),
                        }

                        println!("[INFO] ADB 清理完成，现在请求应用退出。");
                        app_handle_clone.exit(0);
                    });
                }
            }
            _ => {}
        });
}
