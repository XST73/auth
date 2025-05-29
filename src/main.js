const { invoke } = window.__TAURI__.core;
const { open, save } = window.__TAURI__.dialog;
const { listen } = window.__TAURI__.event;
const { BaseDirectory, readTextFile, writeFile, removeFile, createDir, exists } = window.__TAURI__.fs; // FS API
const { appDataDir, join } = window.__TAURI__.path; // Path API


const logOutput = document.getElementById('logOutput');
const refreshDevicesBtn = document.getElementById('refreshDevices');
const deviceListDiv = document.getElementById('deviceList');
const authorizeAndroidBtn = document.getElementById('authorizeAndroid');
const batchModeCheckbox = document.getElementById('batchModeCheckbox');

const generateWindowsDeviceCodeBtn = document.getElementById('generateWindowsDeviceCode');
const currentDeviceCodeSpan = document.getElementById('currentDeviceCode');
const deviceCodeFilePathDisplay = document.getElementById('deviceCodeFilePathDisplay');
const selectAuthLicenseDirBtn = document.getElementById('selectAuthLicenseDir');
const authFileDirDisplay = document.getElementById('authFileDirDisplay');
const authorizeWindowsBtn = document.getElementById('authorizeWindowsBtn');

const selectLicenseToVerifyBtn = document.getElementById('selectLicenseToVerify');
const selectDeviceCodeForVerifyBtn = document.getElementById('selectDeviceCodeForVerify');
const verifyWindowsLicenseBtn = document.getElementById('verifyWindowsLicenseBtn');
const verificationResultP = document.getElementById('verificationResult');

let selectedDeviceCodeForAuth = null;
let selectedAuthFileDir = null;
let selectedLicenseFileToVerifyPath = null;
let selectedDeviceCodeFileToVerifyPath = null;

function appendLog(message) {
  logOutput.textContent += message + '\n';
  logOutput.scrollTop = logOutput.scrollHeight;
}

// 监听来自 Rust 后端的日志事件
listen('log_message', (event) => {
  appendLog(event.payload);
});

// Tab 切换逻辑
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
// 默认打开第一个tab
document.querySelector('.tab-button').click();


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
  authorizeAndroidBtn.addEventListener('click', async () => {
    const batchMode = batchModeCheckbox.checked;
    appendLog(`开始 Android 授权 (批量: ${batchMode})...`);
    try {
      const result = await invoke('process_android_authorization', { batchMode });
      appendLog('Android 授权结果: ' + result);
    } catch (error) {
      appendLog('Android 授权错误: ' + error);
    }
  });
}

// --- Windows Tab Logic ---
if (generateWindowsDeviceCodeBtn) {
  generateWindowsDeviceCodeBtn.addEventListener('click', async () => {
    appendLog('正在生成 Windows 设备码...');
    try {
      const deviceCode = await invoke('generate_windows_device_code');
      currentDeviceCodeSpan.textContent = deviceCode;
      selectedDeviceCodeForAuth = deviceCode; // 存储设备码用于后续授权
      appendLog('Windows 设备码已生成: ' + deviceCode);

      // 询问用户是否要将此设备码保存到文件
      const confirmed = await window.__TAURI__.dialog.ask(
          `设备码已生成: ${deviceCode}\n\n是否要将其保存到 device_code.bin 文件中？`,
          { title: '保存设备码', type: 'info' }
      );

      if (confirmed) {
        const filePath = await save({
          defaultPath: 'device_code.bin',
          filters: [{ name: 'Device Code File', extensions: ['bin'] }]
        });
        if (filePath) {
          await writeFile({ path: filePath, contents: deviceCode });
          deviceCodeFilePathDisplay.textContent = filePath;
          appendLog(`设备码已保存到: ${filePath}`);
        } else {
          appendLog('用户取消保存设备码文件。');
        }
      }
      updateWindowsAuthButtonState();
    } catch (error) {
      currentDeviceCodeSpan.textContent = '生成失败';
      appendLog('生成 Windows 设备码错误: ' + error);
    }
  });
}

if (selectAuthLicenseDirBtn) {
  selectAuthLicenseDirBtn.addEventListener('click', async () => {
    try {
      const dir = await open({ directory: true, multiple: false, title: "选择授权文件生成目录" });
      if (dir) {
        selectedAuthFileDir = dir;
        authFileDirDisplay.textContent = dir;
        appendLog(`授权文件生成目录选定: ${dir}`);
      } else {
        appendLog('用户取消选择目录。');
      }
      updateWindowsAuthButtonState();
    } catch (error) {
      appendLog('选择目录错误: ' + error);
    }
  });
}

function updateWindowsAuthButtonState() {
  authorizeWindowsBtn.disabled = !(selectedDeviceCodeForAuth && selectedAuthFileDir);
}
updateWindowsAuthButtonState(); // Initial check

if (authorizeWindowsBtn) {
  authorizeWindowsBtn.addEventListener('click', async () => {
    if (!selectedDeviceCodeForAuth || !selectedAuthFileDir) {
      appendLog('请先生成设备码并选择授权文件目录。');
      return;
    }
    appendLog(`开始 Windows 授权，设备码: ${selectedDeviceCodeForAuth}, 路径: ${selectedAuthFileDir}`);
    try {
      const result = await invoke('generate_auth_file_cmd', {
        deviceCode: selectedDeviceCodeForAuth,
        targetPathStr: selectedAuthFileDir
      });
      appendLog('Windows 授权成功: ' + result);
      alert('Windows 授权成功！详情请查看日志。');
    } catch (error) {
      appendLog('Windows 授权错误: ' + error);
      alert('Windows 授权失败: ' + error);
    }
  });
}

// Verification part
if (selectLicenseToVerifyBtn) {
  selectLicenseToVerifyBtn.addEventListener('click', async () => {
    const filePath = await open({
      multiple: false,
      filters: [{ name: 'License File', extensions: ['lic'] }]
    });
    if (filePath) {
      selectedLicenseFileToVerifyPath = filePath;
      document.getElementById('selectLicenseToVerify').textContent = `已选授权文件: ...${filePath.slice(-20)}`;
      appendLog(`选择待验证授权文件: ${filePath}`);
    }
    updateVerifyButtonState();
  });
}

if (selectDeviceCodeForVerifyBtn) {
  selectDeviceCodeForVerifyBtn.addEventListener('click', async () => {
    const filePath = await open({
      multiple: false,
      filters: [{ name: 'Device Code File', extensions: ['bin'] }]
    });
    if (filePath) {
      selectedDeviceCodeFileToVerifyPath = filePath;
      document.getElementById('selectDeviceCodeForVerify').textContent = `已选设备码文件: ...${filePath.slice(-20)}`;
      appendLog(`选择待验证设备码文件: ${filePath}`);
    }
    updateVerifyButtonState();
  });
}

function updateVerifyButtonState() {
  verifyWindowsLicenseBtn.disabled = !(selectedLicenseFileToVerifyPath && selectedDeviceCodeFileToVerifyPath);
}
updateVerifyButtonState();

if (verifyWindowsLicenseBtn) {
  verifyWindowsLicenseBtn.addEventListener('click', async () => {
    if (!selectedLicenseFileToVerifyPath || !selectedDeviceCodeFileToVerifyPath) {
      verificationResultP.textContent = '请先选择授权文件和设备码文件。';
      return;
    }
    appendLog(`开始验证授权文件: ${selectedLicenseFileToVerifyPath}，设备码文件: ${selectedDeviceCodeFileToVerifyPath}`);
    verificationResultP.textContent = '验证中...';
    try {
      const authData = await invoke('check_authorization_cmd', {
        authFilePathStr: selectedLicenseFileToVerifyPath,
        deviceCodeFilePathStr: selectedDeviceCodeFileToVerifyPath
      });
      verificationResultP.textContent = `验证通过! 设备码: ${authData.device_code}, 序列号: ${authData.serial_number}, 时间: ${authData.issued_at}`;
      appendLog('验证成功: ' + JSON.stringify(authData));
    } catch (error) {
      verificationResultP.textContent = '验证失败: ' + error;
      appendLog('验证错误: ' + error);
    }
  });
}

appendLog('前端脚本已加载。应用准备就绪。');
