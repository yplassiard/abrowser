//! Model download functionality
//!
//! Downloads GGUF models from HuggingFace and saves them to the models directory.

use std::path::PathBuf;
use tokio::io::AsyncWriteExt;

/// Model download information
#[derive(Debug, Clone)]
pub struct ModelDownload {
    pub id: String,
    pub name: String,
    pub url: String,
    pub filename: String,
    pub size_bytes: u64,
}

/// Available models for download
pub fn available_downloads() -> Vec<ModelDownload> {
    vec![
        // Vision models
        ModelDownload {
            id: "llava-v1.5-7b-q4".to_string(),
            name: "LLaVA 1.5 7B (Q4)".to_string(),
            url: "https://huggingface.co/mys/ggml_llava-v1.5-7b/resolve/main/ggml-model-q4_k.gguf".to_string(),
            filename: "llava-v1.5-7b-q4.gguf".to_string(),
            size_bytes: 4_368_438_944,
        },
        ModelDownload {
            id: "moondream2-q4".to_string(),
            name: "Moondream2 (Q4)".to_string(),
            url: "https://huggingface.co/vikhyatk/moondream2/resolve/main/moondream2-text-model-f16.gguf".to_string(),
            filename: "moondream2-q4.gguf".to_string(),
            size_bytes: 3_500_000_000,
        },
        // Text models
        ModelDownload {
            id: "gemma-2b-q4".to_string(),
            name: "Gemma 2B (Q4) - Light".to_string(),
            url: "https://huggingface.co/lmstudio-ai/gemma-2b-it-GGUF/resolve/main/gemma-2b-it-q4_k_m.gguf".to_string(),
            filename: "gemma-2b-q4.gguf".to_string(),
            size_bytes: 1_500_000_000,
        },
        ModelDownload {
            id: "gemma-7b-q4".to_string(),
            name: "Gemma 7B (Q4) - Medium".to_string(),
            url: "https://huggingface.co/lmstudio-ai/gemma-7b-it-GGUF/resolve/main/gemma-7b-it-q4_k_m.gguf".to_string(),
            filename: "gemma-7b-q4.gguf".to_string(),
            size_bytes: 5_000_000_000,
        },
        ModelDownload {
            id: "llama-3-8b-q4".to_string(),
            name: "Llama 3 8B (Q4) - Large".to_string(),
            url: "https://huggingface.co/QuantFactory/Meta-Llama-3-8B-Instruct-GGUF/resolve/main/Meta-Llama-3-8B-Instruct.Q4_K_M.gguf".to_string(),
            filename: "llama-3-8b-q4.gguf".to_string(),
            size_bytes: 4_920_000_000,
        },
    ]
}

/// Get download info for a model ID
pub fn get_download_info(model_id: &str) -> Option<ModelDownload> {
    available_downloads().into_iter().find(|m| m.id == model_id)
}

/// Download progress callback
pub type ProgressCallback = Box<dyn Fn(u64, u64) + Send + Sync>;

/// Download a model to the models directory
pub async fn download_model(
    model_id: &str,
    models_dir: &PathBuf,
    progress_callback: Option<ProgressCallback>,
) -> Result<PathBuf, String> {
    let download = get_download_info(model_id)
        .ok_or_else(|| format!("Unknown model: {}", model_id))?;

    // Create models directory if needed
    tokio::fs::create_dir_all(models_dir)
        .await
        .map_err(|e| format!("Failed to create models directory: {}", e))?;

    let dest_path = models_dir.join(&download.filename);
    let temp_path = models_dir.join(format!("{}.downloading", download.filename));

    // Check if already downloaded
    if dest_path.exists() {
        return Ok(dest_path);
    }

    // Start download
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(3600)) // 1 hour timeout for large files
        .build()
        .map_err(|e| format!("Failed to create HTTP client: {}", e))?;

    let response = client
        .get(&download.url)
        .send()
        .await
        .map_err(|e| format!("Failed to start download: {}", e))?;

    if !response.status().is_success() {
        return Err(format!("Download failed: HTTP {}", response.status()));
    }

    let total_size = response.content_length().unwrap_or(download.size_bytes);

    // Open temp file for writing
    let mut file = tokio::fs::File::create(&temp_path)
        .await
        .map_err(|e| format!("Failed to create file: {}", e))?;

    // Download with progress
    let mut downloaded: u64 = 0;
    let mut stream = response.bytes_stream();

    use futures_util::StreamExt;
    while let Some(chunk_result) = stream.next().await {
        let chunk = chunk_result.map_err(|e| format!("Download error: {}", e))?;

        file.write_all(&chunk)
            .await
            .map_err(|e| format!("Write error: {}", e))?;

        downloaded += chunk.len() as u64;

        if let Some(ref callback) = progress_callback {
            callback(downloaded, total_size);
        }
    }

    file.flush()
        .await
        .map_err(|e| format!("Flush error: {}", e))?;

    // Rename temp file to final name
    tokio::fs::rename(&temp_path, &dest_path)
        .await
        .map_err(|e| format!("Failed to rename file: {}", e))?;

    Ok(dest_path)
}

/// Download status
#[derive(Debug, Clone)]
pub enum DownloadStatus {
    /// Not started
    Idle,
    /// Downloading (downloaded bytes, total bytes)
    Downloading(u64, u64),
    /// Completed
    Completed(PathBuf),
    /// Failed
    Failed(String),
}

/// Model downloader that tracks progress
pub struct ModelDownloader {
    models_dir: PathBuf,
    status: std::sync::Arc<tokio::sync::RwLock<std::collections::HashMap<String, DownloadStatus>>>,
}

impl ModelDownloader {
    pub fn new(models_dir: PathBuf) -> Self {
        Self {
            models_dir,
            status: std::sync::Arc::new(tokio::sync::RwLock::new(std::collections::HashMap::new())),
        }
    }

    /// Get current download status for a model
    pub async fn get_status(&self, model_id: &str) -> DownloadStatus {
        self.status
            .read()
            .await
            .get(model_id)
            .cloned()
            .unwrap_or(DownloadStatus::Idle)
    }

    /// Start downloading a model (non-blocking)
    pub fn start_download(&self, model_id: String) -> tokio::task::JoinHandle<Result<PathBuf, String>> {
        let models_dir = self.models_dir.clone();
        let status = self.status.clone();
        let model_id_clone = model_id.clone();

        tokio::spawn(async move {
            // Set status to downloading
            {
                let mut s = status.write().await;
                s.insert(model_id_clone.clone(), DownloadStatus::Downloading(0, 0));
            }

            let status_clone = status.clone();
            let model_id_for_callback = model_id_clone.clone();

            let callback: ProgressCallback = Box::new(move |downloaded, total| {
                let status = status_clone.clone();
                let model_id = model_id_for_callback.clone();
                // Update status (fire and forget)
                tokio::spawn(async move {
                    let mut s = status.write().await;
                    s.insert(model_id, DownloadStatus::Downloading(downloaded, total));
                });
            });

            match download_model(&model_id_clone, &models_dir, Some(callback)).await {
                Ok(path) => {
                    let mut s = status.write().await;
                    s.insert(model_id_clone, DownloadStatus::Completed(path.clone()));
                    Ok(path)
                }
                Err(e) => {
                    let mut s = status.write().await;
                    s.insert(model_id_clone, DownloadStatus::Failed(e.clone()));
                    Err(e)
                }
            }
        })
    }

    /// Check if a model is already downloaded
    pub fn is_downloaded(&self, model_id: &str) -> bool {
        if let Some(download) = get_download_info(model_id) {
            self.models_dir.join(&download.filename).exists()
        } else {
            false
        }
    }

    /// List downloaded models
    pub fn list_downloaded(&self) -> Vec<String> {
        available_downloads()
            .into_iter()
            .filter(|d| self.models_dir.join(&d.filename).exists())
            .map(|d| d.id)
            .collect()
    }
}
