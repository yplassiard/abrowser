//! Local llama.cpp backend for AI inference
//!
//! Uses llama-cpp-2 for local model inference without external servers.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::RwLock;

use super::{AiBackend, AiError, AiResult};

/// Local llama.cpp backend for AI inference
pub struct LocalBackend {
    /// Directory containing GGUF model files
    models_dir: PathBuf,
    /// Result cache
    cache: Arc<RwLock<HashMap<String, String>>>,
    /// Number of GPU layers to offload (0 = CPU only)
    n_gpu_layers: i32,
}

impl LocalBackend {
    /// Create a new local backend
    pub fn new(models_dir: PathBuf) -> AiResult<Self> {
        // Initialize llama backend (this is required before any llama operations)
        llama_cpp_2::llama_backend::LlamaBackend::init()
            .map_err(|e| AiError::ModelLoad(format!("Failed to init llama backend: {}", e)))?;

        Ok(Self {
            models_dir,
            cache: Arc::new(RwLock::new(HashMap::new())),
            n_gpu_layers: 0, // CPU only by default
        })
    }

    /// Create with GPU acceleration
    pub fn with_gpu(models_dir: PathBuf, n_gpu_layers: u32) -> AiResult<Self> {
        let mut backend = Self::new(models_dir)?;
        backend.n_gpu_layers = n_gpu_layers as i32;
        Ok(backend)
    }

    /// Find the model file path
    fn find_model_path(&self, model_id: &str) -> AiResult<PathBuf> {
        // Try exact match first
        let exact = self.models_dir.join(format!("{}.gguf", model_id));
        if exact.exists() {
            return Ok(exact);
        }

        // Try with common suffixes
        for suffix in &["Q4_K_M", "Q4_K_S", "Q4_0", "Q5_K_M", "Q8_0", "f16"] {
            let path = self.models_dir.join(format!("{}-{}.gguf", model_id, suffix));
            if path.exists() {
                return Ok(path);
            }
        }

        // Scan directory for partial matches
        if let Ok(entries) = std::fs::read_dir(&self.models_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_lowercase();
                if name.contains(&model_id.to_lowercase()) && name.ends_with(".gguf") {
                    return Ok(entry.path());
                }
            }
        }

        Err(AiError::ModelNotFound(format!(
            "Model '{}' not found in {:?}. Download a GGUF model file.",
            model_id, self.models_dir
        )))
    }

    /// List available models
    pub fn list_models(&self) -> Vec<String> {
        let mut models = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&self.models_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().to_string();
                if name.ends_with(".gguf") {
                    models.push(name.trim_end_matches(".gguf").to_string());
                }
            }
        }
        models
    }

    /// Run inference synchronously (to be called from spawn_blocking)
    fn run_inference(&self, model_path: PathBuf, prompt: String, max_tokens: usize) -> AiResult<String> {
        use llama_cpp_2::context::params::LlamaContextParams;
        use llama_cpp_2::llama_backend::LlamaBackend;
        use llama_cpp_2::llama_batch::LlamaBatch;
        use llama_cpp_2::model::params::LlamaModelParams;
        use llama_cpp_2::model::LlamaModel;
        use llama_cpp_2::sampling::LlamaSampler;

        // Re-init backend for this thread
        let backend = LlamaBackend::init()
            .map_err(|e| AiError::ModelLoad(format!("Backend init failed: {}", e)))?;

        // Load model
        let model_params = LlamaModelParams::default();
        let model = LlamaModel::load_from_file(&backend, &model_path, &model_params)
            .map_err(|e| AiError::ModelLoad(format!("Failed to load model: {}", e)))?;

        // Create context
        let ctx_params = LlamaContextParams::default();
        let mut ctx = model
            .new_context(&backend, ctx_params)
            .map_err(|e| AiError::Inference(format!("Failed to create context: {}", e)))?;

        // Tokenize prompt
        let tokens = model
            .str_to_token(&prompt, llama_cpp_2::model::AddBos::Always)
            .map_err(|e| AiError::Inference(format!("Tokenization failed: {}", e)))?;

        if tokens.is_empty() {
            return Err(AiError::Inference("Empty prompt".to_string()));
        }

        // Create batch
        let mut batch = LlamaBatch::new(2048, 1);

        // Add prompt tokens to batch
        for (i, token) in tokens.iter().enumerate() {
            let is_last = i == tokens.len() - 1;
            batch.add(*token, i as i32, &[0], is_last)
                .map_err(|e| AiError::Inference(format!("Batch add failed: {}", e)))?;
        }

        // Decode prompt
        ctx.decode(&mut batch)
            .map_err(|e| AiError::Inference(format!("Decode failed: {}", e)))?;

        // Create sampler for text generation
        let mut sampler = LlamaSampler::chain_simple([
            LlamaSampler::temp(0.7),
            LlamaSampler::dist(42),
        ]);

        // Generate tokens
        let mut output = String::new();
        let mut n_cur = tokens.len() as i32;

        for _ in 0..max_tokens {
            // Sample next token
            let new_token = sampler.sample(&ctx, batch.n_tokens() - 1);

            // Check for end of generation
            if model.is_eog_token(new_token) {
                break;
            }

            // Decode token to string
            let piece = model.token_to_str(new_token, llama_cpp_2::model::Special::Tokenize)
                .map_err(|e| AiError::Inference(format!("Token decode failed: {}", e)))?;
            output.push_str(&piece);

            // Prepare next batch
            batch.clear();
            batch.add(new_token, n_cur, &[0], true)
                .map_err(|e| AiError::Inference(format!("Batch add failed: {}", e)))?;

            // Decode
            ctx.decode(&mut batch)
                .map_err(|e| AiError::Inference(format!("Decode failed: {}", e)))?;

            n_cur += 1;
        }

        Ok(output.trim().to_string())
    }
}

#[async_trait::async_trait]
impl AiBackend for LocalBackend {
    fn name(&self) -> &str {
        "local"
    }

    async fn is_available(&self) -> bool {
        // Check if any models are available
        !self.list_models().is_empty()
    }

    async fn describe_image(&self, image_bytes: &[u8], model: &str) -> AiResult<String> {
        // Find the vision model path
        let _model_path = self.find_model_path(model)?;

        // Decode image to get dimensions (for context in prompt)
        let img = image::load_from_memory(image_bytes)
            .map_err(|e| AiError::Inference(format!("Failed to decode image: {}", e)))?;

        let (width, height) = (img.width(), img.height());

        // Note: Full LLaVA support requires multimodal processing
        // This simplified version creates a text-only prompt
        // For actual vision support, we'd need llava-specific code
        let _prompt = format!(
            "USER: Describe this {}x{} image very briefly in one short sentence for a blind user. Focus on the main subject and action.\nASSISTANT:",
            width, height
        );

        // Run inference in blocking task
        let result = tokio::task::spawn_blocking(move || {
            // For now, return a helpful message about vision model requirements
            // Full LLaVA inference requires special image embedding handling
            Ok::<String, AiError>(format!(
                "Vision model inference requires LLaVA with image processing. Image size: {}x{}",
                width, height
            ))
        })
        .await
        .map_err(|e| AiError::Inference(format!("Task failed: {}", e)))??;

        Ok(result)
    }

    async fn generate_text(&self, prompt: &str, model_id: &str) -> AiResult<String> {
        let model_path = self.find_model_path(model_id)?;
        let prompt = prompt.to_string();

        // Run inference in blocking task since llama-cpp types aren't Send
        tokio::task::spawn_blocking(move || {
            use llama_cpp_2::context::params::LlamaContextParams;
            use llama_cpp_2::llama_backend::LlamaBackend;
            use llama_cpp_2::llama_batch::LlamaBatch;
            use llama_cpp_2::model::params::LlamaModelParams;
            use llama_cpp_2::model::LlamaModel;
            use llama_cpp_2::sampling::LlamaSampler;

            // Init backend for this thread
            let backend = LlamaBackend::init()
                .map_err(|e| AiError::ModelLoad(format!("Backend init failed: {}", e)))?;

            // Load model
            let model_params = LlamaModelParams::default();
            let model = LlamaModel::load_from_file(&backend, &model_path, &model_params)
                .map_err(|e| AiError::ModelLoad(format!("Failed to load model: {}", e)))?;

            // Create context
            let ctx_params = LlamaContextParams::default();
            let mut ctx = model
                .new_context(&backend, ctx_params)
                .map_err(|e| AiError::Inference(format!("Failed to create context: {}", e)))?;

            // Tokenize prompt
            let tokens = model
                .str_to_token(&prompt, llama_cpp_2::model::AddBos::Always)
                .map_err(|e| AiError::Inference(format!("Tokenization failed: {}", e)))?;

            if tokens.is_empty() {
                return Err(AiError::Inference("Empty prompt".to_string()));
            }

            // Create batch
            let mut batch = LlamaBatch::new(2048, 1);

            // Add prompt tokens
            for (i, token) in tokens.iter().enumerate() {
                let is_last = i == tokens.len() - 1;
                batch.add(*token, i as i32, &[0], is_last)
                    .map_err(|e| AiError::Inference(format!("Batch add failed: {}", e)))?;
            }

            // Decode prompt
            ctx.decode(&mut batch)
                .map_err(|e| AiError::Inference(format!("Decode failed: {}", e)))?;

            // Create sampler
            let mut sampler = LlamaSampler::chain_simple([
                LlamaSampler::temp(0.7),
                LlamaSampler::dist(42),
            ]);

            // Generate tokens
            let mut output = String::new();
            let mut n_cur = tokens.len() as i32;
            let max_tokens = 256;

            for _ in 0..max_tokens {
                let new_token = sampler.sample(&ctx, batch.n_tokens() - 1);

                if model.is_eog_token(new_token) {
                    break;
                }

                let piece = model.token_to_str(new_token, llama_cpp_2::model::Special::Tokenize)
                    .map_err(|e| AiError::Inference(format!("Token decode failed: {}", e)))?;
                output.push_str(&piece);

                batch.clear();
                batch.add(new_token, n_cur, &[0], true)
                    .map_err(|e| AiError::Inference(format!("Batch add failed: {}", e)))?;

                ctx.decode(&mut batch)
                    .map_err(|e| AiError::Inference(format!("Decode failed: {}", e)))?;

                n_cur += 1;
            }

            Ok(output.trim().to_string())
        })
        .await
        .map_err(|e| AiError::Inference(format!("Task join failed: {}", e)))?
    }

    async fn get_cached(&self, key: &str) -> Option<String> {
        self.cache.read().await.get(key).cloned()
    }

    async fn set_cached(&self, key: &str, value: &str) {
        self.cache.write().await.insert(key.to_string(), value.to_string());
    }
}
