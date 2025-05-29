const { invoke } = window.__TAURI__.core;
const { open } = window.__TAURI__.dialog;
const { listen } = window.__TAURI__.event;
const { BaseDirectory, readTextFile, writeFile, removeFile, createDir, exists } = window.__TAURI__.fs;
const { appDir, join, resourceDir, appConfigDir, appLogDir, appDataDir } = window.__TAURI__.path; // Added appDir and other path utilities

const logOutput = document.getElementById('logOutput');
const refreshDevicesBtn = document.getElementById('refreshDevices');
const deviceListDiv = document.getElementById('deviceList');
const authorizeAndroidBtn = document.getElementById('authorizeAndroid');
const batchModeCheckbox = document.getElementById('batchModeCheckbox');

// --- Windows Tab Elements (Revised) ---
const selectAppDirBtn = document.getElementById('selectAppDirBtn');
const appDirPathDisplay = document.getElementById('appDirPathDisplay');
const authorizeWindowsAppBtn = document.getElementById('authorizeWindowsAppBtn');
const windowsAuthResultP = document.getElementById('windowsAuthResult'); // For displaying combined result


let selectedAppDir = null; // For Windows application directory

function appendLog(message) {
  logOutput.textContent += message + '\n';
  logOutput.scrollTop = logOutput.scrollHeight;
}

listen('log_message', (event) => {
  appendLog(event.payload);
});

function openTab(evt, tabName) {
  var i, tabcontent, tablinks;
  tabcontent = document.getElementsByClassName("tab-content");
  for (i = 0; i < tabcontent.length; i++) {
    tabcontent[i].style.display = "none";
  }
  tablinks = document.getElementsByClassName("tab-button");
  for (i = 0; i < tablinks.length; i++) {
    tablinks[i].className = tablinks[i].className.replace(" active", "");
  }
  document.getElementById(tabName).style.display = "block";
  evt.currentTarget.className += " active";
}
// Default open first tab (or based on preference)
document.addEventListener('DOMContentLoaded', async () => {
  document.querySelector('.tab-button').click();

  // Set default application directory for Windows
  try {
    // 调用新的 Rust 命令来获取可执行文件所在的目录
    const exeDir = await invoke('get_executable_dir');
    if (exeDir) {
      selectedAppDir = exeDir;
      appDirPathDisplay.textContent = `${exeDir} (默认)`;
      appendLog(`默认应用程序目录已设置为: ${exeDir}`);
    } else {
      appDirPathDisplay.textContent = '未选择 (请手动选择)';
      appendLog("未能自动获取默认应用目录，请手动选择。");
    }
  } catch (e) {
    appendLog("获取默认应用目录错误: " + e);
    appDirPathDisplay.textContent = '未选择 (请手动选择)';
  }
  updateWindowsAuthorizeAppButtonState(); // 更新按钮状态
});


// --- Android Tab Logic ---
if (refreshDevicesBtn) {
  refreshDevicesBtn.addEventListener('click', async () => {
    appendLog('正在刷新 Android 设备列表...');
    try {
      const devices = await invoke('list_adb_devices');
      deviceListDiv.textContent = '设备列表: ' + (devices.length > 0 ? devices.join(', ') : '无设备连接');
      appendLog('设备列表刷新成功: ' + devices.join(', '));
    } catch (error) {
      deviceListDiv.textContent = '设备列表: 获取失败';
      appendLog('获取设备列表错误: ' + error);
    }
  });
}

if (authorizeAndroidBtn) {
  authorizeAndroidBtn.addEventListener("click", async () => {
    const batchMode = batchModeCheckbox.checked; // 保留批量模式的读取
    appendLog(`开始 Android 授权 (批量模式: ${batchMode})...`);
    authorizeAndroidBtn.disabled = true;
    try {
      // 将 batchMode 传递给后端
      const result = await invoke("process_android_authorization", {
        batchMode,
      });
      appendLog("Android 授权结果: \n" + result); //  \n for better multi-line display
      alert("Android 设备授权操作完成！详情请查看日志。");
    } catch (error) {
      appendLog("Android 授权错误: " + error);
      alert("Android 设备授权失败: " + error);
    } finally {
      authorizeAndroidBtn.disabled = false;
      if (!batchMode) {
        // 如果是单设备模式
        appendLog("可以刷新设备列表或连接下一个设备后再次点击授权。");
      } else {
        // 如果是批量模式
        appendLog("批量授权完成。如需操作新设备，请先连接并刷新列表。");
      }
    }
  });
}

// --- Windows Tab Logic (Revised) ---

if (selectAppDirBtn) {
  selectAppDirBtn.addEventListener('click', async () => {
    try {
      const dir = await open({ directory: true, multiple: false, title: "选择应用程序根目录" });
      if (dir) {
        selectedAppDir = dir;
        appDirPathDisplay.textContent = dir;
        appendLog(`应用程序目录选定: ${dir}`);
      } else {
        appendLog('用户取消选择目录。');
        // selectedAppDir = null; // Keep previous if cancelled, or clear if desired
        // appDirPathDisplay.textContent = '未选择';
      }
      updateWindowsAuthorizeAppButtonState();
    } catch (error) {
      appendLog('选择目录错误: ' + error);
    }
  });
}

function updateWindowsAuthorizeAppButtonState() {
  authorizeWindowsAppBtn.disabled = !selectedAppDir;
}
// Call on init, after DOMContentLoaded if selectedAppDir might be set by default
// For now, called after DOMContentLoaded if default logic is added there.
// If no default is set, this correctly disables button until selection.


if (authorizeWindowsAppBtn) {
  authorizeWindowsAppBtn.addEventListener('click', async () => {
    if (!selectedAppDir) {
      appendLog('请先选择应用程序目录。');
      windowsAuthResultP.textContent = '请先选择应用程序目录。';
      return;
    }
    appendLog(`开始 Windows 应用授权与校验，应用目录: ${selectedAppDir}`);
    windowsAuthResultP.textContent = '正在授权与校验...';
    authorizeWindowsAppBtn.disabled = true; // Disable button during operation

    try {
      const result = await invoke('authorize_windows_application', {
        applicationPathStr: selectedAppDir
      });
      appendLog('Windows 应用授权与校验完成。');
      appendLog(`授权消息: ${result.authorization_message}`);
      appendLog(`校验状态: ${result.verification_status}`);

      windowsAuthResultP.textContent = `授权: ${result.authorization_message}. 校验: ${result.verification_status}`;
      if (result.verification_details) {
        appendLog(`校验详情: 设备码=${result.verification_details.device_code}, 序列号=${result.verification_details.serial_number}, 时间=${result.verification_details.issued_at}`);
        windowsAuthResultP.textContent += ` (设备码: ${result.verification_details.device_code})`;
      }
      // Optionally show an alert or more prominent UI update for overall success/failure
      if (result.verification_status.includes("通过")) {
        alert('Windows 应用授权和校验成功！详情请查看日志和结果区域。');
      } else {
        alert(`Windows 应用授权成功，但自动校验失败或部分成功。详情: ${result.verification_status}`);
      }

    } catch (error) {
      appendLog('Windows 应用授权与校验错误: ' + error);
      windowsAuthResultP.textContent = '授权与校验失败: ' + error;
      alert('Windows 应用授权与校验失败: ' + error);
    } finally {
      updateWindowsAuthorizeAppButtonState(); // Re-enable or keep disabled based on state
    }
  });
}

// Initial state for the button
updateWindowsAuthorizeAppButtonState();


appendLog('前端脚本已加载。应用准备就绪。');