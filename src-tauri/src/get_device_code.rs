use sha2::{Digest, Sha256};
use std::fs;
use std::io::Write;
use std::process::Command;
use std::str;

fn get_motherboard_serial() -> Option<String> {
    let output = Command::new("wmic")
        .args(&["baseboard", "get", "serialnumber"])
        .output()
        .expect("Failed to execute command");

    if output.status.success() {
        let stdout = str::from_utf8(&output.stdout).unwrap();
        let serial_number = stdout.lines().nth(1).unwrap_or("").trim();
        if !serial_number.is_empty() {
            return Some(serial_number.to_string());
        }
    }
    None
}

pub fn generate_device_code() -> Option<String> {
    if let Some(serial_number) = get_motherboard_serial() {
        let mut hasher = Sha256::new();
        hasher.update(serial_number.as_bytes());
        let result = hasher.finalize();
        let device_code = result
            .iter()
            .map(|b| format!("{:02x}", b))
            .collect::<String>();
        let device_code = &device_code[..16]; // 取前16位作为设备码

        fs::write("device_code.bin", device_code.as_bytes()).unwrap_or_default();

        return Some(device_code.to_string());
    }
    None
}
