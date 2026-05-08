use futures::StreamExt;
use notify_rust::Notification;
use reqwest::Client;
use serde::Serialize;
use std::env;
use std::fs::File;
use std::io::Write;
use std::net::TcpListener;
use std::path::Path;
use tauri::path::BaseDirectory;
use tauri::{AppHandle, Emitter, Manager};

#[tauri::command]
pub async fn open_url(_: tauri::AppHandle, url: String) {
    open::that(url).unwrap();
}

#[tauri::command]
pub async fn run_command(command: String) -> Result<String, String> {
    #[cfg(target_os = "windows")]
    let output = tokio::process::Command::new("powershell")
        .arg("-Command")
        .arg(&command)
        .creation_flags(0x08000000)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    #[cfg(not(target_os = "windows"))]
    let output = tokio::process::Command::new("sh")
        .arg("-c")
        .arg(&command)
        .output()
        .await
        .map_err(|e| e.to_string())?;

    if output.status.success() {
        #[cfg(target_os = "windows")]
        {
            use encoding_rs::GBK;
            let (decoded, _, _) = GBK.decode(&output.stdout);
            Ok(decoded.into_owned())
        }
        #[cfg(not(target_os = "windows"))]
        {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        }
    } else {
        #[cfg(target_os = "windows")]
        {
            use encoding_rs::GBK;
            let (decoded, _, _) = GBK.decode(&output.stderr);
            Err(decoded.into_owned())
        }
        #[cfg(not(target_os = "windows"))]
        {
            Err(String::from_utf8_lossy(&output.stderr).to_string())
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadProgress {
    file_id: String,
    downloaded: u64,
    total: u64,
}

#[tauri::command]
pub async fn download_file(
    app: AppHandle,
    url: String,
    save_path: String,
    file_id: String,
) -> Result<(), String> {
    let client = Client::new();
    let resp = client.get(&url).send().await.map_err(|e| e.to_string())?;
    // if save path is empty
    let mut save_path = save_path;
    let file_name = url.split('/').last().unwrap();
    if save_path.is_empty() {
        let file_path = app
            .path()
            .resolve(file_name, BaseDirectory::Download)
            .expect("failed to resolve resource");
        save_path = file_path.to_str().unwrap().to_string();
    } else {
        let file_path = Path::new(&save_path).join(file_name);
        save_path = file_path.to_str().unwrap().to_string();
    }
    // if file exists, add number to file name
    if Path::new(&save_path).exists() {
        let mut i = 1;
        while Path::new(&save_path).exists() {
            save_path = save_path.split('.').nth(0).unwrap().to_string()
                + &i.to_string()
                + "."
                + save_path.split('.').nth(1).unwrap();
            i += 1;
        }
    }
    let total_size = resp.content_length();
    let mut stream = resp.bytes_stream();
    let mut file = File::create(&save_path).map_err(|e| e.to_string())?;
    let mut downloaded: u64 = 0;
    println!("save_path: {}", save_path);
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        file.write_all(&chunk).map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        app.emit(
            "download_progress",
            DownloadProgress {
                file_id: file_id.clone(),
                downloaded,
                total: total_size.unwrap_or(0),
            },
        )
        .unwrap();
    }
    Ok(())
}

#[tauri::command]
pub fn get_exe_dir() -> String {
    let exe_path = env::current_exe().unwrap();
    let exe_dir = exe_path.parent().unwrap();
    exe_dir.to_str().unwrap().to_string()
}

#[tauri::command]
pub fn get_env_var(name: String) -> Result<String, String> {
    std::env::var(name).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn find_port() -> Result<u16, String> {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    Ok(port)
}

#[derive(serde::Deserialize)]
pub struct NotificationParams {
    title: String,
    body: String,
    icon: String,
}

#[tauri::command]
pub fn notification(app: AppHandle, params: NotificationParams) -> Result<(), String> {
    let mut notifi_app = Notification::new();
    #[cfg(target_os = "macos")]
    {
        let _ = notify_rust::set_application(if tauri::is_dev() {
            "com.apple.Terminal"
        } else {
            &app.config().identifier
        });
    }
    #[cfg(windows)]
    {
        use std::path::MAIN_SEPARATOR as SEP;
        let curr_dir = get_exe_dir(true);
        // set the notification's System.AppUserModel.ID only when running the installed app
        if !(curr_dir.ends_with(format!("{SEP}target{SEP}debug").as_str())
            || curr_dir.ends_with(format!("{SEP}target{SEP}release").as_str()))
        {
            notifi_app.app_id(&app.config().identifier);
        }
    }
    if !params.icon.is_empty() {
        notifi_app.icon(&params.icon);
    } else {
        notifi_app.auto_icon();
    }
    tauri::async_runtime::spawn(async move {
        let _ = notifi_app
            .summary(&params.title)
            .body(&params.body)
            .show()
            .expect("show notification failed");
    });
    Ok(())
}

#[tauri::command]
pub fn get_machine_uid() -> String {
    let uid: String = machine_uid::get().unwrap();
    uid
}
