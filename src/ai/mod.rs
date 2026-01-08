//! AI module for image description and text generation
//!
//! Supports multiple backends:
//! - Ollama (REST API, requires external server)
//! - Local (llama.cpp, runs models directly)

mod ollama;

pub use ollama::OllamaBackend;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

/// AI inference errors
#[derive(Debug, Clone)]
pub enum AiError {
    /// Backend not available
    NotAvailable,
    /// Model not found
    ModelNotFound(String),
    /// Network error
    Network(String),
    /// Inference error
    Inference(String),
    /// Parse error
    Parse(String),
    /// Model loading error
    ModelLoad(String),
}

impl std::fmt::Display for AiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AiError::NotAvailable => write!(f, "AI backend not available"),
            AiError::ModelNotFound(m) => write!(f, "Model not found: {}", m),
            AiError::Network(e) => write!(f, "Network error: {}", e),
            AiError::Inference(e) => write!(f, "Inference error: {}", e),
            AiError::Parse(e) => write!(f, "Parse error: {}", e),
            AiError::ModelLoad(e) => write!(f, "Model load error: {}", e),
        }
    }
}

impl std::error::Error for AiError {}

pub type AiResult<T> = Result<T, AiError>;

/// Trait for AI backends
#[async_trait::async_trait]
pub trait AiBackend: Send + Sync {
    /// Backend name
    fn name(&self) -> &str;

    /// Check if backend is available
    async fn is_available(&self) -> bool;

    /// Describe an image
    async fn describe_image(&self, image_bytes: &[u8], model: &str) -> AiResult<String>;

    /// Generate text from prompt
    async fn generate_text(&self, prompt: &str, model: &str) -> AiResult<String>;

    /// Get cached result
    async fn get_cached(&self, key: &str) -> Option<String>;

    /// Set cached result
    async fn set_cached(&self, key: &str, value: &str);
}

/// Model information
#[derive(Debug, Clone)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub size_bytes: u64,
    pub downloaded: bool,
    pub capabilities: Vec<ModelCapability>,
}

/// Model capabilities
#[derive(Debug, Clone, PartialEq)]
pub enum ModelCapability {
    /// Can describe images
    Vision,
    /// Can generate/summarize text
    Text,
    /// Can extract structured data
    Extraction,
}

/// Available models for download
pub fn available_models() -> Vec<ModelInfo> {
    vec![
        ModelInfo {
            id: "llava-v1.5-7b-q4".to_string(),
            name: "LLaVA 1.5 7B (Q4)".to_string(),
            size_bytes: 4_294_967_296, // ~4GB
            downloaded: false,
            capabilities: vec![ModelCapability::Vision, ModelCapability::Text],
        },
        ModelInfo {
            id: "gemma-3-4b-q4".to_string(),
            name: "Gemma 3 4B (Q4)".to_string(),
            size_bytes: 2_684_354_560, // ~2.5GB
            downloaded: false,
            capabilities: vec![ModelCapability::Text, ModelCapability::Extraction],
        },
    ]
}

/// AI service that manages backends and caching
pub struct AiService {
    /// Active backend
    backend: Arc<dyn AiBackend>,
    /// Description cache (image_url -> description)
    cache: Arc<RwLock<HashMap<String, String>>>,
    /// Models directory
    models_dir: PathBuf,
}

impl AiService {
    /// Create with Ollama backend
    pub fn with_ollama(endpoint: Option<String>) -> Self {
        let models_dir = dirs::data_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("abrowser")
            .join("models");

        Self {
            backend: Arc::new(OllamaBackend::new(endpoint)),
            cache: Arc::new(RwLock::new(HashMap::new())),
            models_dir,
        }
    }

    /// Check if AI is available
    pub async fn is_available(&self) -> bool {
        self.backend.is_available().await
    }

    /// Get backend name
    pub fn backend_name(&self) -> &str {
        self.backend.name()
    }

    /// Describe an image from bytes
    pub async fn describe_image(&self, image_bytes: &[u8], cache_key: &str) -> AiResult<String> {
        // Check cache
        if let Some(cached) = self.cache.read().await.get(cache_key) {
            return Ok(cached.clone());
        }

        // Call backend
        let result = self.backend.describe_image(image_bytes, "llava").await?;

        // Cache result
        self.cache.write().await.insert(cache_key.to_string(), result.clone());

        Ok(result)
    }

    /// Describe an image from URL
    pub async fn describe_image_url(&self, url: &str) -> AiResult<String> {
        // Check cache
        if let Some(cached) = self.cache.read().await.get(url) {
            return Ok(cached.clone());
        }

        // Fetch image
        let client = reqwest::Client::new();
        let response = client.get(url).send().await
            .map_err(|e| AiError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AiError::Network(format!("HTTP {}", response.status())));
        }

        let bytes = response.bytes().await
            .map_err(|e| AiError::Network(e.to_string()))?;

        self.describe_image(&bytes, url).await
    }

    /// Generate text (for summaries, etc.)
    pub async fn generate_text(&self, prompt: &str, model: &str) -> AiResult<String> {
        self.backend.generate_text(prompt, model).await
    }

    /// Summarize page content
    pub async fn summarize_page(&self, content: &str) -> AiResult<String> {
        let prompt = format!(
            "Summarize this web page content in 2-3 sentences for a blind user:\n\n{}",
            content.chars().take(4000).collect::<String>()
        );
        self.backend.generate_text(&prompt, "gemma3").await
    }

    /// Get models directory
    pub fn models_dir(&self) -> &PathBuf {
        &self.models_dir
    }

    /// Clear cache
    pub async fn clear_cache(&self) {
        self.cache.write().await.clear();
    }
}

// Keep the old ImageDescriber for backward compatibility
pub struct ImageDescriber {
    service: AiService,
}

impl ImageDescriber {
    pub fn new(endpoint: Option<String>, _model: Option<String>) -> Self {
        Self {
            service: AiService::with_ollama(endpoint),
        }
    }

    pub async fn is_available(&self) -> bool {
        self.service.is_available().await
    }

    pub async fn get_cached(&self, url: &str) -> Option<String> {
        self.service.cache.read().await.get(url).cloned()
    }

    pub async fn describe_from_url(&self, image_url: &str) -> Option<String> {
        self.service.describe_image_url(image_url).await.ok()
    }

    pub async fn describe_bytes(&self, image_bytes: &[u8], cache_key: &str) -> Option<String> {
        self.service.describe_image(image_bytes, cache_key).await.ok()
    }

    pub async fn clear_cache(&self) {
        self.service.clear_cache().await;
    }

    pub async fn reset_availability(&self) {
        // No-op for now, availability is checked each time
    }
}

impl Default for ImageDescriber {
    fn default() -> Self {
        Self::new(None, None)
    }
}
