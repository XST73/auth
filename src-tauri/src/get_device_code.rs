// use sha2::{Digest, Sha256};
// use std::process::Command;
// use std::str;
// 
// fn get_motherboard_serial() -> Option<String> {
//     let output = Command::new("wmic")
//         .args(&["baseboard", "get", "serialnumber"])
//         .output();
// 
//     match output {
//         Ok(out) => {
//             if out.status.success() {
//                 let stdout = str::from_utf8(&out.stdout).unwrap_or_default(); 
//                 let serial_number = stdout.lines().nth(1).unwrap_or("").trim();
//                 if !serial_number.is_empty() && serial_number != "To be filled by O.E.M." {
//                     return Some(serial_number.to_string());
//                 }
//             }
//             None
//         }
//         Err(_) => None,
//     }
// }
// 
// pub fn generate_device_code() -> Result<String, String> {
//     if let Some(serial_number) = get_motherboard_serial() {
//         let mut hasher = Sha256::new();
//         hasher.update(serial_number.as_bytes());
//         let result = hasher.finalize();
//         let device_code_full = result
//             .iter()
//             .map(|b| format!("{:02x}", b))
//             .collect::<String>();
//         let device_code = &device_code_full[..16]; // 取前16位作为设备码
//         
// 
//         Ok(device_code.to_string())
//     } else {
//         Err("无法获取主板序列号或序列号无效".to_string())
//     }
// }