//! Ollama backend for AI inference
//!
//! Uses the Ollama REST API for image description and text generation.

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::{AiBackend, AiError, AiResult};

/// Ollama API request for generation
#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    images: Vec<String>,
    stream: bool,
}

/// Ollama API response
#[derive(Debug, Deserialize)]
struct OllamaResponse {
    response: String,
}

/// Ollama backend for AI inference
pub struct OllamaBackend {
    endpoint: String,
    client: reqwest::Client,
    cache: Arc<RwLock<HashMap<String, String>>>,
    available: Arc<RwLock<Option<bool>>>,
}

impl OllamaBackend {
    pub fn new(endpoint: Option<String>) -> Self {
        Self {
            endpoint: endpoint.unwrap_or_else(|| "http://localhost:11434".to_string()),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(60))
                .build()
                .unwrap_or_default(),
            cache: Arc::new(RwLock::new(HashMap::new())),
            available: Arc::new(RwLock::new(None)),
        }
    }

    async fn check_availability(&self) -> bool {
        let url = format!("{}/api/tags", self.endpoint);
        match self.client.get(&url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// List available models
    pub async fn list_models(&self) -> AiResult<Vec<String>> {
        let url = format!("{}/api/tags", self.endpoint);
        let response = self.client.get(&url).send().await
            .map_err(|e| AiError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AiError::NotAvailable);
        }

        #[derive(Deserialize)]
        struct TagsResponse {
            models: Vec<ModelInfo>,
        }
        #[derive(Deserialize)]
        struct ModelInfo {
            name: String,
        }

        let tags: TagsResponse = response.json().await
            .map_err(|e| AiError::Parse(e.to_string()))?;

        Ok(tags.models.into_iter().map(|m| m.name).collect())
    }
}

#[async_trait::async_trait]
impl AiBackend for OllamaBackend {
    fn name(&self) -> &str {
        "ollama"
    }

    async fn is_available(&self) -> bool {
        {
            let available = self.available.read().await;
            if let Some(avail) = *available {
                return avail;
            }
        }
        let avail = self.check_availability().await;
        *self.available.write().await = Some(avail);
        avail
    }

    async fn describe_image(&self, image_bytes: &[u8], model: &str) -> AiResult<String> {
        if !self.is_available().await {
            return Err(AiError::NotAvailable);
        }

        let base64_image = base64::engine::general_purpose::STANDARD.encode(image_bytes);

        let request = OllamaRequest {
            model: model.to_string(),
            prompt: "Describe this image very briefly in one short sentence for a blind user. Focus on the main subject and action.".to_string(),
            images: vec![base64_image],
            stream: false,
        };

        let url = format!("{}/api/generate", self.endpoint);
        let response = self.client.post(&url).json(&request).send().await
            .map_err(|e| AiError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AiError::Inference(format!("HTTP {}", response.status())));
        }

        let ollama_response: OllamaResponse = response.json().await
            .map_err(|e| AiError::Parse(e.to_string()))?;

        Ok(ollama_response.response.trim().to_string())
    }

    async fn generate_text(&self, prompt: &str, model: &str) -> AiResult<String> {
        if !self.is_available().await {
            return Err(AiError::NotAvailable);
        }

        let request = OllamaRequest {
            model: model.to_string(),
            prompt: prompt.to_string(),
            images: vec![],
            stream: false,
        };

        let url = format!("{}/api/generate", self.endpoint);
        let response = self.client.post(&url).json(&request).send().await
            .map_err(|e| AiError::Network(e.to_string()))?;

        if !response.status().is_success() {
            return Err(AiError::Inference(format!("HTTP {}", response.status())));
        }

        let ollama_response: OllamaResponse = response.json().await
            .map_err(|e| AiError::Parse(e.to_string()))?;

        Ok(ollama_response.response.trim().to_string())
    }

    async fn get_cached(&self, key: &str) -> Option<String> {
        self.cache.read().await.get(key).cloned()
    }

    async fn set_cached(&self, key: &str, value: &str) {
        self.cache.write().await.insert(key.to_string(), value.to_string());
    }
}
