//! AI-powered image description using Ollama/LLaVA
//!
//! Provides optional image descriptions for accessibility when images lack alt text.

use base64::Engine;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// Ollama API request for image description
#[derive(Debug, Serialize)]
struct OllamaRequest {
    model: String,
    prompt: String,
    images: Vec<String>,
    stream: bool,
}

/// Ollama API response
#[derive(Debug, Deserialize)]
struct OllamaResponse {
    response: String,
}

/// AI image describer using Ollama with LLaVA
pub struct ImageDescriber {
    /// Ollama endpoint URL
    endpoint: String,
    /// Model to use (default: llava)
    model: String,
    /// HTTP client
    client: reqwest::Client,
    /// Cache of descriptions by image URL
    cache: Arc<RwLock<HashMap<String, String>>>,
    /// Whether the service is available
    available: Arc<RwLock<Option<bool>>>,
}

impl ImageDescriber {
    /// Create a new image describer
    pub fn new(endpoint: Option<String>, model: Option<String>) -> Self {
        Self {
            endpoint: endpoint.unwrap_or_else(|| "http://localhost:11434".to_string()),
            model: model.unwrap_or_else(|| "llava".to_string()),
            client: reqwest::Client::builder()
                .timeout(std::time::Duration::from_secs(30))
                .build()
                .unwrap_or_default(),
            cache: Arc::new(RwLock::new(HashMap::new())),
            available: Arc::new(RwLock::new(None)),
        }
    }

    /// Check if Ollama is available (cached check)
    pub async fn is_available(&self) -> bool {
        // Check cached availability
        {
            let available = self.available.read().await;
            if let Some(avail) = *available {
                return avail;
            }
        }

        // Perform availability check
        let avail = self.check_availability().await;
        *self.available.write().await = Some(avail);
        avail
    }

    /// Actually check if Ollama is running
    async fn check_availability(&self) -> bool {
        let url = format!("{}/api/tags", self.endpoint);
        match self.client.get(&url).send().await {
            Ok(resp) => resp.status().is_success(),
            Err(_) => false,
        }
    }

    /// Get cached description for an image URL
    pub async fn get_cached(&self, url: &str) -> Option<String> {
        self.cache.read().await.get(url).cloned()
    }

    /// Describe an image from URL
    pub async fn describe_from_url(&self, image_url: &str) -> Option<String> {
        // Check cache first
        if let Some(cached) = self.get_cached(image_url).await {
            return Some(cached);
        }

        // Check if service is available
        if !self.is_available().await {
            return None;
        }

        // Fetch the image
        let image_bytes = match self.client.get(image_url).send().await {
            Ok(resp) if resp.status().is_success() => match resp.bytes().await {
                Ok(bytes) => bytes,
                Err(_) => return None,
            },
            _ => return None,
        };

        self.describe_bytes(&image_bytes, image_url).await
    }

    /// Describe an image from raw bytes
    pub async fn describe_bytes(&self, image_bytes: &[u8], cache_key: &str) -> Option<String> {
        // Check cache first
        if let Some(cached) = self.get_cached(cache_key).await {
            return Some(cached);
        }

        // Check if service is available
        if !self.is_available().await {
            return None;
        }

        // Base64 encode the image
        let base64_image = base64::engine::general_purpose::STANDARD.encode(image_bytes);

        // Build request
        let request = OllamaRequest {
            model: self.model.clone(),
            prompt: "Describe this image very briefly in one short sentence for a blind user. Focus on the main subject and action. Do not start with 'This image shows' or 'The image depicts'.".to_string(),
            images: vec![base64_image],
            stream: false,
        };

        // Send request
        let url = format!("{}/api/generate", self.endpoint);
        let response = match self.client.post(&url).json(&request).send().await {
            Ok(resp) => resp,
            Err(_) => return None,
        };

        if !response.status().is_success() {
            return None;
        }

        // Parse response
        let ollama_response: OllamaResponse = match response.json().await {
            Ok(r) => r,
            Err(_) => return None,
        };

        let description = ollama_response.response.trim().to_string();

        // Cache the result
        if !description.is_empty() {
            self.cache
                .write()
                .await
                .insert(cache_key.to_string(), description.clone());
        }

        Some(description)
    }

    /// Clear the description cache
    pub async fn clear_cache(&self) {
        self.cache.write().await.clear();
    }

    /// Reset availability check (e.g., after config change)
    pub async fn reset_availability(&self) {
        *self.available.write().await = None;
    }
}

impl Default for ImageDescriber {
    fn default() -> Self {
        Self::new(None, None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_describer_creation() {
        let describer = ImageDescriber::new(None, None);
        assert_eq!(describer.endpoint, "http://localhost:11434");
        assert_eq!(describer.model, "llava");
    }

    #[tokio::test]
    async fn test_cache() {
        let describer = ImageDescriber::new(None, None);

        // Manually insert into cache
        describer
            .cache
            .write()
            .await
            .insert("test_url".to_string(), "test description".to_string());

        // Should retrieve from cache
        let cached = describer.get_cached("test_url").await;
        assert_eq!(cached, Some("test description".to_string()));
    }
}
