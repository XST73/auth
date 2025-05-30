const { invoke } = window.__TAURI__.core;
const { open } = window.__TAURI__.dialog;
const { getCurrentWindow } = window.__TAURI__.window; // Import appWindow for window controls
const { listen } = window.__TAURI__.event; //
// fs and path imports are not directly used in the provided snippet for this change, but keep them if used elsewhere.

// --- UI Elements ---
// const logOutput = document.getElementById('logOutput'); // Removed
const refreshDevicesBtn = document.getElementById('refreshDevices');
const deviceListDiv = document.getElementById('deviceList');
const authorizeAndroidBtn = document.getElementById('authorizeAndroid');
const batchModeCheckbox = document.getElementById('batchModeCheckbox');

const selectAppDirBtn = document.getElementById('selectAppDirBtn');
const appDirPathDisplay = document.getElementById('appDirPathDisplay');
const authorizeWindowsAppBtn = document.getElementById('authorizeWindowsAppBtn');
const windowsAuthResultP = document.getElementById('windowsAuthResult');

// New Status/Progress Bar Elements
const statusText = document.getElementById('statusText');
const progressBar = document.getElementById('progressBar');

const minimizeBtn = document.getElementById('titlebar-minimize');
const maximizeBtn = document.getElementById('titlebar-maximize');
const closeBtn = document.getElementById('titlebar-close');

const appWindow = getCurrentWindow();

let selectedAppDir = null;

// --- Logging and Status Updates ---
function updateStatus(message, isError = false) {
    if (statusText) {
        statusText.textContent = message;
        statusText.className = isError ? 'status-text error' : 'status-text';
    }
    console.log(message); // Keep console log for debugging
}

// This function will replace appendLog for UI feedback.
// Detailed logs will still go to console via `updateStatus` and backend `log_to_frontend`
// We might not need to show every single log message in the status bar.
// function appendLog(message) { // Original function
//   logOutput.textContent += message + '\n';
//   logOutput.scrollTop = logOutput.scrollHeight;
// }

listen('log_message', (event) => { //
                                   // For now, we'll just log these to the console.
                                   // If specific messages from backend need to update status bar,
                                   // we can parse event.payload or use different event types.
    console.log("[Rust]: " + event.payload);
});


// --- Progress Bar Functions ---
function showProgress() {
    if (progressBar) {
        progressBar.style.display = 'block';
        progressBar.value = 0; // Reset progress
    }
}

function hideProgress() {
    if (progressBar) {
        progressBar.style.display = 'none';
    }
}

function updateProgress(value) { // value from 0 to 100
    if (progressBar) {
        progressBar.value = value;
    }
}


// --- Tab Management ---
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

// --- Initialization ---
document.addEventListener('DOMContentLoaded', async () => {
    const windowsTabButton = Array.from(document.querySelectorAll('.tab-button')).find(btn => btn.textContent.includes('Windows 授权'));
    if (windowsTabButton) {
        windowsTabButton.click();
    } else {
        document.querySelector('.tab-button').click();
    }

    try {
        const exeDir = await invoke('get_executable_dir');
        if (exeDir) {
            selectedAppDir = exeDir;
            appDirPathDisplay.textContent = `${exeDir} (默认)`;
            updateStatus(`默认应用程序目录已设置为: ${exeDir}`);
        } else {
            appDirPathDisplay.textContent = '未选择 (请手动选择)';
            updateStatus("未能自动获取默认应用目录，请手动选择。", true);
        }
    } catch (e) {
        updateStatus("获取默认应用目录错误: " + e, true);
        appDirPathDisplay.textContent = '未选择 (请手动选择)';
    }
    updateWindowsAuthorizeAppButtonState();
    updateStatus('前端脚本已加载。应用准备就绪。');

    if (minimizeBtn) {
        minimizeBtn.addEventListener('click', () => appWindow.minimize());
    }
    if (maximizeBtn) {
        maximizeBtn.addEventListener('click', () => appWindow.toggleMaximize());
    }
    if (closeBtn) {
        closeBtn.addEventListener('click', () => appWindow.close());
    }
});


// --- Android Tab Logic ---
if (refreshDevicesBtn) {
    refreshDevicesBtn.addEventListener('click', async () => {
        updateStatus('正在刷新 Android 设备列表...');
        showProgress(); // Show progress for this async operation
        updateProgress(30); // Example: indeterminate start
        try {
            const devices = await invoke('list_adb_devices');
            deviceListDiv.textContent = (devices.length > 0 ? devices.join(', ') : '无设备连接');
            updateStatus('设备列表刷新成功: ' + (devices.length > 0 ? devices.join(', ') : '无设备连接'));
            updateProgress(100);
        } catch (error) {
            deviceListDiv.textContent = '获取失败';
            updateStatus('获取设备列表错误: ' + error, true);
            updateProgress(100); // Still complete the "progress" even if error
        } finally {
            setTimeout(hideProgress, 500); // Hide after a short delay
        }
    });
}

if (authorizeAndroidBtn) {
    authorizeAndroidBtn.addEventListener("click", async () => {
        const batchMode = batchModeCheckbox.checked;
        updateStatus(`开始 Android 授权 (批量模式: ${batchMode})...`);
        authorizeAndroidBtn.disabled = true;
        showProgress();
        updateProgress(10); // Initial progress

        try {
            // Example of how you might update progress if the backend could send events
            // For now, just simulating some progress
            await new Promise(resolve => setTimeout(resolve, 500)); // Simulate work
            updateProgress(50);

            const result = await invoke("process_android_authorization", { batchMode }); //
            updateStatus("Android 授权操作完成!"); // Simpler status update
            console.log("Android 授权结果: \n" + result); // Detailed result to console
            alert("Android 设备授权操作完成！详细信息请查看控制台日志。");
            updateProgress(100);
        } catch (error) {
            updateStatus("Android 授权错误: " + error, true);
            alert("Android 设备授权失败: " + error);
            updateProgress(100); // Complete progress
        } finally {
            authorizeAndroidBtn.disabled = false;
            setTimeout(hideProgress, 500);
            if (!batchMode) {
                updateStatus("单设备授权完成。刷新设备或连接下一个设备后可再次授权。");
            } else {
                updateStatus("批量授权完成。如需操作新设备，请先连接并刷新列表。");
            }
        }
    });
}

// --- Windows Tab Logic ---
if (selectAppDirBtn) {
    selectAppDirBtn.addEventListener('click', async () => {
        try {
            const dir = await open({ directory: true, multiple: false, title: "选择应用程序根目录" }); //
            if (dir) {
                selectedAppDir = dir;
                appDirPathDisplay.textContent = dir;
                updateStatus(`应用程序目录选定: ${dir}`);
            } else {
                updateStatus('用户取消选择目录。');
            }
            updateWindowsAuthorizeAppButtonState();
        } catch (error) {
            updateStatus('选择目录错误: ' + error, true);
        }
    });
}

function updateWindowsAuthorizeAppButtonState() {
    if (authorizeWindowsAppBtn) { // Ensure button exists
        authorizeWindowsAppBtn.disabled = !selectedAppDir;
    }
}

if (authorizeWindowsAppBtn) {
    authorizeWindowsAppBtn.addEventListener('click', async () => {
        if (!selectedAppDir) {
            updateStatus('请先选择应用程序目录。', true);
            windowsAuthResultP.textContent = '请先选择应用程序目录。';
            return;
        }
        updateStatus(`开始 Windows 应用授权与校验，应用目录: ${selectedAppDir}`);
        windowsAuthResultP.textContent = '正在授权与校验...';
        authorizeWindowsAppBtn.disabled = true;
        showProgress();
        updateProgress(10);

        try {
            // Simulate some work
            await new Promise(resolve => setTimeout(resolve, 500));
            updateProgress(30);

            const result = await invoke('authorize_windows_application', { //
                applicationPathStr: selectedAppDir
            });

            updateProgress(80);
            updateStatus('Windows 应用授权与校验完成。');
            console.log(`授权消息: ${result.authorization_message}`); //
            console.log(`校验状态: ${result.verification_status}`); //

            windowsAuthResultP.textContent = `授权: ${result.authorization_message}. \n校验: ${result.verification_status}`; //
            if (result.verification_details) {
                console.log(`校验详情: 设备码=${result.verification_details.device_code}, 序列号=${result.verification_details.serial_number}, 时间=${result.verification_details.issued_at}`); //
                windowsAuthResultP.textContent += ` (设备码: ${result.verification_details.device_code})`; //
            }

            if (result.verification_status.includes("通过")) { //
                alert('Windows 应用授权和校验成功！详细信息请查看控制台日志和结果区域。');
            } else {
                alert(`Windows 应用授权成功，但自动校验失败或部分成功。详情: ${result.verification_status}`); //
            }
            updateProgress(100);
        } catch (error) {
            updateStatus('Windows 应用授权与校验错误: ' + error, true);
            windowsAuthResultP.textContent = '授权与校验失败: ' + error; //
            alert('Windows 应用授权与校验失败: ' + error);
            updateProgress(100);
        } finally {
            updateWindowsAuthorizeAppButtonState();
            setTimeout(hideProgress, 500);
        }
    });
}

// Initial state for the button
updateWindowsAuthorizeAppButtonState();