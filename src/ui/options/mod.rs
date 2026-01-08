//! Options dialog for AI model management
//!
//! Provides HTML/JS interface for:
//! - Viewing available models
//! - Downloading models from HuggingFace
//! - Configuring AI settings

use crate::ai::{available_models, ModelCapability, ModelInfo};
use std::path::PathBuf;

/// Generate the options HTML page
pub fn generate_options_html(models_dir: &PathBuf) -> String {
    let models = available_models();
    let models_json = generate_models_json(&models, models_dir);

    format!(
        r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>abrowser Options</title>
    <style>
        :root {{
            --bg-primary: #1a1a2e;
            --bg-secondary: #16213e;
            --bg-card: #0f3460;
            --text-primary: #eee;
            --text-secondary: #aaa;
            --accent: #e94560;
            --accent-hover: #ff6b6b;
            --success: #4ecca3;
            --border: #333;
        }}

        * {{
            box-sizing: border-box;
            margin: 0;
            padding: 0;
        }}

        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: var(--bg-primary);
            color: var(--text-primary);
            line-height: 1.6;
            padding: 2rem;
        }}

        h1 {{
            color: var(--accent);
            margin-bottom: 0.5rem;
        }}

        .subtitle {{
            color: var(--text-secondary);
            margin-bottom: 2rem;
        }}

        .section {{
            background: var(--bg-secondary);
            border-radius: 8px;
            padding: 1.5rem;
            margin-bottom: 1.5rem;
        }}

        .section h2 {{
            color: var(--text-primary);
            margin-bottom: 1rem;
            font-size: 1.2rem;
        }}

        .model-card {{
            background: var(--bg-card);
            border-radius: 6px;
            padding: 1rem;
            margin-bottom: 1rem;
            display: flex;
            justify-content: space-between;
            align-items: center;
        }}

        .model-info h3 {{
            color: var(--text-primary);
            margin-bottom: 0.25rem;
        }}

        .model-info .capabilities {{
            display: flex;
            gap: 0.5rem;
            margin-top: 0.5rem;
        }}

        .capability {{
            background: var(--bg-secondary);
            padding: 0.2rem 0.5rem;
            border-radius: 4px;
            font-size: 0.8rem;
            color: var(--text-secondary);
        }}

        .capability.vision {{
            color: var(--accent);
            border: 1px solid var(--accent);
        }}

        .capability.text {{
            color: var(--success);
            border: 1px solid var(--success);
        }}

        .model-size {{
            color: var(--text-secondary);
            font-size: 0.9rem;
        }}

        .model-actions {{
            display: flex;
            flex-direction: column;
            gap: 0.5rem;
            align-items: flex-end;
        }}

        button {{
            background: var(--accent);
            color: white;
            border: none;
            padding: 0.5rem 1rem;
            border-radius: 4px;
            cursor: pointer;
            font-size: 0.9rem;
            transition: background 0.2s;
        }}

        button:hover {{
            background: var(--accent-hover);
        }}

        button:disabled {{
            background: var(--border);
            cursor: not-allowed;
        }}

        button.downloaded {{
            background: var(--success);
        }}

        .progress-bar {{
            width: 150px;
            height: 6px;
            background: var(--border);
            border-radius: 3px;
            overflow: hidden;
            display: none;
        }}

        .progress-bar.active {{
            display: block;
        }}

        .progress-bar .fill {{
            height: 100%;
            background: var(--accent);
            width: 0%;
            transition: width 0.3s;
        }}

        .status {{
            font-size: 0.8rem;
            color: var(--text-secondary);
        }}

        .settings-row {{
            display: flex;
            justify-content: space-between;
            align-items: center;
            padding: 0.75rem 0;
            border-bottom: 1px solid var(--border);
        }}

        .settings-row:last-child {{
            border-bottom: none;
        }}

        select {{
            background: var(--bg-card);
            color: var(--text-primary);
            border: 1px solid var(--border);
            padding: 0.5rem;
            border-radius: 4px;
            font-size: 0.9rem;
        }}

        .backend-status {{
            display: flex;
            align-items: center;
            gap: 0.5rem;
        }}

        .status-dot {{
            width: 10px;
            height: 10px;
            border-radius: 50%;
            background: var(--border);
        }}

        .status-dot.online {{
            background: var(--success);
        }}

        .status-dot.offline {{
            background: var(--accent);
        }}

        footer {{
            margin-top: 2rem;
            text-align: center;
            color: var(--text-secondary);
            font-size: 0.9rem;
        }}

        footer a {{
            color: var(--accent);
            text-decoration: none;
        }}
    </style>
</head>
<body>
    <h1>abrowser Options</h1>
    <p class="subtitle">AI Settings and Model Management</p>

    <div class="section">
        <h2>Backend Status</h2>
        <div class="settings-row">
            <span>Ollama Server</span>
            <div class="backend-status">
                <div class="status-dot" id="ollama-status"></div>
                <span id="ollama-status-text">Checking...</span>
            </div>
        </div>
        <div class="settings-row">
            <span>Local Models</span>
            <div class="backend-status">
                <div class="status-dot" id="local-status"></div>
                <span id="local-status-text">Not configured</span>
            </div>
        </div>
    </div>

    <div class="section">
        <h2>AI Models</h2>
        <div id="models-list"></div>
    </div>

    <div class="section">
        <h2>Settings</h2>
        <div class="settings-row">
            <label for="vision-model">Image Description Model</label>
            <select id="vision-model">
                <option value="llava">LLaVA (via Ollama)</option>
                <option value="llava-local">LLaVA (Local)</option>
            </select>
        </div>
        <div class="settings-row">
            <label for="text-model">Text Generation Model</label>
            <select id="text-model">
                <option value="gemma3">Gemma 3 (via Ollama)</option>
                <option value="gemma3-local">Gemma 3 (Local)</option>
            </select>
        </div>
    </div>

    <footer>
        <p>abrowser - Accessible Terminal Browser</p>
        <p><a href="https://github.com/yplassiard/abrowser">GitHub</a></p>
    </footer>

    <script>
        const MODELS = {models_json};
        const MODELS_DIR = "{models_dir}";

        // Check Ollama status
        async function checkOllama() {{
            const dot = document.getElementById('ollama-status');
            const text = document.getElementById('ollama-status-text');
            try {{
                const resp = await fetch('http://localhost:11434/api/tags');
                if (resp.ok) {{
                    const data = await resp.json();
                    dot.classList.add('online');
                    dot.classList.remove('offline');
                    text.textContent = `Online (${{data.models?.length || 0}} models)`;
                }} else {{
                    throw new Error('Not OK');
                }}
            }} catch (e) {{
                dot.classList.add('offline');
                dot.classList.remove('online');
                text.textContent = 'Offline - run: ollama serve';
            }}
        }}

        // Render model cards
        function renderModels() {{
            const container = document.getElementById('models-list');
            container.innerHTML = MODELS.map(model => `
                <div class="model-card" data-id="${{model.id}}">
                    <div class="model-info">
                        <h3>${{model.name}}</h3>
                        <div class="model-size">${{formatSize(model.size_bytes)}}</div>
                        <div class="capabilities">
                            ${{model.capabilities.map(c =>
                                `<span class="capability ${{c.toLowerCase()}}">${{c}}</span>`
                            ).join('')}}
                        </div>
                    </div>
                    <div class="model-actions">
                        <button
                            onclick="downloadModel('${{model.id}}')"
                            class="${{model.downloaded ? 'downloaded' : ''}}"
                            ${{model.downloaded ? 'disabled' : ''}}
                        >
                            ${{model.downloaded ? '✓ Downloaded' : 'Download'}}
                        </button>
                        <div class="progress-bar" id="progress-${{model.id}}">
                            <div class="fill"></div>
                        </div>
                        <div class="status" id="status-${{model.id}}"></div>
                    </div>
                </div>
            `).join('');
        }}

        function formatSize(bytes) {{
            const gb = bytes / (1024 * 1024 * 1024);
            return `${{gb.toFixed(1)}} GB`;
        }}

        async function downloadModel(modelId) {{
            const progressBar = document.getElementById(`progress-${{modelId}}`);
            const fill = progressBar.querySelector('.fill');
            const status = document.getElementById(`status-${{modelId}}`);
            const button = document.querySelector(`[data-id="${{modelId}}"] button`);

            progressBar.classList.add('active');
            button.disabled = true;
            button.textContent = 'Downloading...';

            // Model URLs (HuggingFace)
            const urls = {{
                'llava-v1.5-7b-q4': 'https://huggingface.co/mys/ggml_llava-v1.5-7b/resolve/main/ggml-model-q4_k.gguf',
                'gemma-3-4b-q4': 'https://huggingface.co/google/gemma-3-4b-it-qat-q4_0-gguf/resolve/main/gemma-3-4b-it-q4_0.gguf'
            }};

            const url = urls[modelId];
            if (!url) {{
                status.textContent = 'Unknown model';
                return;
            }}

            try {{
                status.textContent = 'Starting download...';

                // In a real implementation, we'd use a Rust backend for this
                // For now, show instructions
                status.textContent = 'Use: ollama pull llava';
                button.textContent = 'See instructions';
                progressBar.classList.remove('active');

            }} catch (e) {{
                status.textContent = `Error: ${{e.message}}`;
                button.disabled = false;
                button.textContent = 'Retry';
                progressBar.classList.remove('active');
            }}
        }}

        // Initialize
        checkOllama();
        renderModels();

        // Refresh status periodically
        setInterval(checkOllama, 5000);
    </script>
</body>
</html>"#,
        models_json = models_json,
        models_dir = models_dir.display()
    )
}

fn generate_models_json(models: &[ModelInfo], models_dir: &PathBuf) -> String {
    let models_with_status: Vec<serde_json::Value> = models
        .iter()
        .map(|m| {
            let model_path = models_dir.join(format!("{}.gguf", m.id));
            let downloaded = model_path.exists();

            serde_json::json!({
                "id": m.id,
                "name": m.name,
                "size_bytes": m.size_bytes,
                "downloaded": downloaded,
                "capabilities": m.capabilities.iter().map(|c| match c {
                    ModelCapability::Vision => "Vision",
                    ModelCapability::Text => "Text",
                    ModelCapability::Extraction => "Extract",
                }).collect::<Vec<_>>()
            })
        })
        .collect();

    serde_json::to_string(&models_with_status).unwrap_or_else(|_| "[]".to_string())
}

/// Open options in browser
pub fn open_options_data_url(models_dir: &PathBuf) -> String {
    let html = generate_options_html(models_dir);
    let encoded = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, html.as_bytes());
    format!("data:text/html;base64,{}", encoded)
}
