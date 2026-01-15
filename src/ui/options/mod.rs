//! Options dialog for AI model management
//!
//! Provides HTML/JS interface for:
//! - Viewing available models
//! - Downloading models from HuggingFace
//! - Configuring AI and rendering settings

use crate::ai::{available_downloads, ModelCapability, ModelInfo};
use crate::shell::Config;
use std::path::PathBuf;

/// Available AI models (from download module)
pub fn available_models() -> Vec<ModelInfo> {
    available_downloads()
        .into_iter()
        .map(|d| {
            // Determine capabilities from model name
            let capabilities = if d.id.contains("llava") || d.id.contains("moondream") {
                vec![ModelCapability::Vision, ModelCapability::Text]
            } else {
                vec![ModelCapability::Text, ModelCapability::Extraction]
            };

            ModelInfo {
                id: d.id,
                name: d.name,
                size_bytes: d.size_bytes,
                downloaded: false, // Will be updated in generate_models_json
                capabilities,
            }
        })
        .collect()
}

/// Generate the options HTML page
pub fn generate_options_html(models_dir: &PathBuf, config: &Config) -> String {
    let models = available_models();
    let models_json = generate_models_json(&models, models_dir);
    let viewport_mode = config.viewport_mode.as_str();

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
            --border: #444;
        }}

        * {{ box-sizing: border-box; margin: 0; padding: 0; }}

        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            background: var(--bg-primary);
            color: var(--text-primary);
            line-height: 1.6;
            padding: 2rem;
        }}

        h1 {{ color: var(--accent); margin-bottom: 0.5rem; }}
        .subtitle {{ color: var(--text-secondary); margin-bottom: 0.5rem; }}
        .auto-save {{ color: var(--success); font-size: 0.9rem; margin-bottom: 2rem; }}

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
            border-bottom: 1px solid var(--border);
            padding-bottom: 0.5rem;
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

        .model-info h3 {{ color: var(--text-primary); margin-bottom: 0.25rem; }}
        .model-size {{ color: var(--text-secondary); font-size: 0.9rem; }}

        .capabilities {{
            display: flex;
            gap: 0.5rem;
            margin-top: 0.5rem;
        }}

        .capability {{
            background: var(--bg-secondary);
            padding: 0.2rem 0.5rem;
            border-radius: 4px;
            font-size: 0.8rem;
        }}
        .capability.vision {{ color: var(--accent); border: 1px solid var(--accent); }}
        .capability.text {{ color: var(--success); border: 1px solid var(--success); }}

        button {{
            background: var(--accent);
            color: white;
            border: none;
            padding: 0.5rem 1rem;
            border-radius: 4px;
            cursor: pointer;
            font-size: 0.9rem;
        }}
        button:hover {{ background: var(--accent-hover); }}
        button:disabled {{ background: var(--border); cursor: not-allowed; }}
        button.downloaded {{ background: var(--success); }}
        button.downloading {{ background: #f39c12; animation: pulse 1.5s infinite; }}

        @keyframes pulse {{
            0%, 100% {{ opacity: 1; }}
            50% {{ opacity: 0.7; }}
        }}

        .progress-bar {{
            margin-top: 0.5rem;
            background: var(--bg-secondary);
            border-radius: 4px;
            height: 20px;
            position: relative;
            overflow: hidden;
        }}
        .progress-fill {{
            height: 100%;
            background: linear-gradient(90deg, var(--accent), var(--success));
            transition: width 0.3s ease;
        }}
        .progress-text {{
            position: absolute;
            top: 50%;
            left: 50%;
            transform: translate(-50%, -50%);
            font-size: 0.75rem;
            color: white;
            text-shadow: 0 0 2px black;
        }}

        .settings-row {{
            display: flex;
            justify-content: space-between;
            align-items: center;
            padding: 0.75rem 0;
            border-bottom: 1px solid var(--border);
        }}
        .settings-row:last-child {{ border-bottom: none; }}

        .settings-row label {{
            display: flex;
            align-items: center;
            gap: 0.5rem;
        }}

        /* Accessible select with marker */
        .select-wrapper {{
            position: relative;
            display: inline-block;
        }}
        .select-wrapper::before {{
            content: "[▼ ";
            color: var(--accent);
            pointer-events: none;
        }}
        .select-wrapper::after {{
            content: "]";
            color: var(--accent);
            pointer-events: none;
        }}

        select {{
            background: var(--bg-card);
            color: var(--text-primary);
            border: 1px solid var(--border);
            padding: 0.5rem 2rem 0.5rem 0.5rem;
            border-radius: 4px;
            font-size: 0.9rem;
            appearance: none;
            cursor: pointer;
        }}
        select:focus {{
            outline: 2px solid var(--accent);
            outline-offset: 2px;
        }}

        /* Accessible checkbox with marker */
        .checkbox-wrapper {{
            display: flex;
            align-items: center;
            gap: 0.5rem;
            cursor: pointer;
        }}
        .checkbox-wrapper input[type="checkbox"] {{
            display: none;
        }}
        .checkbox-marker {{
            font-family: monospace;
            font-size: 1.1rem;
            color: var(--accent);
        }}
        .checkbox-wrapper input:checked + .checkbox-marker::before {{
            content: "[x] ";
        }}
        .checkbox-wrapper input:not(:checked) + .checkbox-marker::before {{
            content: "[ ] ";
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
        .status-dot.online {{ background: var(--success); }}
        .status-dot.offline {{ background: var(--accent); }}

        footer {{
            margin-top: 2rem;
            text-align: center;
            color: var(--text-secondary);
            font-size: 0.9rem;
        }}
        footer a {{ color: var(--accent); text-decoration: none; }}

        .hidden {{ display: none !important; }}
    </style>
</head>
<body>
    <h1>abrowser Options</h1>
    <p class="subtitle">AI Settings and Model Management</p>
    <p class="auto-save">Options are saved automatically when changed.</p>

    <div class="section">
        <h2>Rendering</h2>
        <div class="settings-row">
            <label class="checkbox-wrapper" onclick="toggleCheckbox('show-images')">
                <input type="checkbox" id="show-images" {show_images_checked} onchange="saveOptions()">
                <span class="checkbox-marker"></span>
                <span>Show images in page</span>
            </label>
        </div>
        <div class="settings-row" id="auto-describe-row">
            <label class="checkbox-wrapper" onclick="toggleCheckbox('auto-describe')">
                <input type="checkbox" id="auto-describe" {auto_describe_checked} onchange="saveOptions()">
                <span class="checkbox-marker"></span>
                <span>Auto-describe images (requires vision model)</span>
            </label>
        </div>
        <div class="settings-row">
            <label for="viewport-mode">Browser viewport</label>
            <span class="select-wrapper">
                <select id="viewport-mode" onchange="saveOptions()">
                    <option value="desktop" {desktop_selected}>Desktop (1920x1080)</option>
                    <option value="mobile" {mobile_selected}>Mobile (375x812)</option>
                </select>
            </span>
        </div>
    </div>

    <div class="section">
        <h2>AI Backend</h2>
        <div class="settings-row">
            <span>Local llama.cpp</span>
            <div class="backend-status">
                <div class="status-dot" id="backend-status"></div>
                <span id="backend-status-text">Checking models...</span>
            </div>
        </div>
        <div class="settings-row">
            <span>Models directory</span>
            <span id="models-dir" style="color: var(--text-secondary); font-size: 0.9rem;">{models_dir}</span>
        </div>
    </div>

    <div class="section">
        <h2>AI Models (Local GGUF)</h2>
        <p style="color: var(--text-secondary); margin-bottom: 1rem; font-size: 0.9rem;">
            Download GGUF models from HuggingFace and place them in the models directory.
        </p>
        <div class="settings-row">
            <label for="vision-model">Image description model</label>
            <span class="select-wrapper">
                <select id="vision-model" onchange="saveOptions()">
                    <option value="llava">LLaVA (Q4_K_M)</option>
                    <option value="moondream">Moondream2 (Q4)</option>
                </select>
            </span>
        </div>
        <div class="settings-row">
            <label for="text-model">Text generation model</label>
            <span class="select-wrapper">
                <select id="text-model" onchange="saveOptions()">
                    <option value="gemma-2b">Gemma 2B (Q4) - Light</option>
                    <option value="gemma-7b">Gemma 7B (Q4) - Medium</option>
                    <option value="llama-3-8b">Llama 3 8B (Q4) - Large</option>
                </select>
            </span>
        </div>
        <div id="models-list"></div>
    </div>

    <footer>
        <p>abrowser - Accessible Terminal Browser</p>
        <p><a href="https://github.com/yplassiard/abrowser">GitHub</a></p>
    </footer>

    <script>
        const MODELS = {models_json};

        function toggleCheckbox(id) {{
            const cb = document.getElementById(id);
            cb.checked = !cb.checked;
            cb.dispatchEvent(new Event('change'));
        }}

        function updateAutoDescribeVisibility() {{
            const showImages = document.getElementById('show-images').checked;
            const row = document.getElementById('auto-describe-row');
            if (showImages) {{
                row.classList.remove('hidden');
            }} else {{
                row.classList.add('hidden');
                document.getElementById('auto-describe').checked = false;
            }}
        }}

        document.getElementById('show-images').addEventListener('change', updateAutoDescribeVisibility);

        function checkBackend() {{
            const dot = document.getElementById('backend-status');
            const text = document.getElementById('backend-status-text');
            const downloadedModels = MODELS.filter(m => m.downloaded);
            if (downloadedModels.length > 0) {{
                dot.classList.add('online');
                dot.classList.remove('offline');
                text.textContent = `Ready (${{downloadedModels.length}} model${{downloadedModels.length > 1 ? 's' : ''}} found)`;
            }} else {{
                dot.classList.add('offline');
                dot.classList.remove('online');
                text.textContent = 'No models found - download GGUF files';
            }}
        }}

        // Track downloading models
        const downloadingModels = new Set();

        function renderModels() {{
            const container = document.getElementById('models-list');
            container.innerHTML = MODELS.map(model => {{
                const isDownloading = downloadingModels.has(model.id);
                const buttonClass = model.downloaded ? 'downloaded' : (isDownloading ? 'downloading' : '');
                const buttonText = model.downloaded ? 'Downloaded' : (isDownloading ? 'Downloading...' : 'Download');
                const buttonDisabled = model.downloaded || isDownloading;

                return `
                <div class="model-card" data-id="${{model.id}}">
                    <div class="model-info">
                        <h3>${{model.name}}</h3>
                        <div class="model-size">${{formatSize(model.size_bytes)}}</div>
                        <div class="capabilities">
                            ${{model.capabilities.map(c =>
                                `<span class="capability ${{c.toLowerCase()}}">${{c}}</span>`
                            ).join('')}}
                        </div>
                        <div class="progress-bar" id="progress-${{model.id}}" style="display: ${{isDownloading ? 'block' : 'none'}}">
                            <div class="progress-fill" id="progress-fill-${{model.id}}" style="width: 0%"></div>
                            <span class="progress-text" id="progress-text-${{model.id}}">0%</span>
                        </div>
                    </div>
                    <button
                        id="btn-${{model.id}}"
                        onclick="downloadModel('${{model.id}}')"
                        class="${{buttonClass}}"
                        ${{buttonDisabled ? 'disabled' : ''}}
                    >
                        ${{buttonText}}
                    </button>
                </div>
            `;
            }}).join('');
        }}

        function formatSize(bytes) {{
            const gb = bytes / (1024 * 1024 * 1024);
            return `${{gb.toFixed(1)}} GB`;
        }}

        function downloadModel(modelId) {{
            // Mark as downloading
            downloadingModels.add(modelId);
            renderModels();

            // Signal abrowser to start download via localStorage (polled by abrowser)
            localStorage.setItem('abrowser_download_cmd', modelId);
            console.log('ABROWSER_DOWNLOAD:' + modelId);
        }}

        // Track download speed
        const downloadStats = {{}};

        function formatSpeed(bytesPerSec) {{
            if (bytesPerSec < 1024) return bytesPerSec.toFixed(0) + ' B/s';
            if (bytesPerSec < 1024 * 1024) return (bytesPerSec / 1024).toFixed(1) + ' KB/s';
            return (bytesPerSec / (1024 * 1024)).toFixed(2) + ' MB/s';
        }}

        // Handle download progress updates from abrowser
        window.updateDownloadProgress = function(modelId, downloaded, total) {{
            const progressBar = document.getElementById('progress-' + modelId);
            const progressFill = document.getElementById('progress-fill-' + modelId);
            const progressText = document.getElementById('progress-text-' + modelId);

            if (progressBar && progressFill && progressText) {{
                progressBar.style.display = 'block';
                const percent = total > 0 ? Math.round((downloaded / total) * 100) : 0;
                progressFill.style.width = percent + '%';

                // Calculate speed
                const now = Date.now();
                let speedText = '';
                if (!downloadStats[modelId]) {{
                    downloadStats[modelId] = {{ lastBytes: downloaded, lastTime: now, speed: 0 }};
                }} else {{
                    const stats = downloadStats[modelId];
                    const timeDelta = (now - stats.lastTime) / 1000; // seconds
                    if (timeDelta > 0.5) {{ // Update speed every 500ms
                        const bytesDelta = downloaded - stats.lastBytes;
                        stats.speed = bytesDelta / timeDelta;
                        stats.lastBytes = downloaded;
                        stats.lastTime = now;
                    }}
                    if (stats.speed > 0) {{
                        speedText = ' @ ' + formatSpeed(stats.speed);
                    }}
                }}

                progressText.textContent = percent + '% (' + formatSize(downloaded) + ' / ' + formatSize(total) + ')' + speedText;
            }}
        }};

        // Handle download completion from abrowser
        window.downloadComplete = function(modelId, success, message) {{
            downloadingModels.delete(modelId);
            delete downloadStats[modelId]; // Clear speed tracking
            if (success) {{
                // Update the model as downloaded
                const model = MODELS.find(m => m.id === modelId);
                if (model) model.downloaded = true;
                checkBackend();
            }} else {{
                alert('Download failed: ' + message);
            }}
            renderModels();
        }};

        function saveOptions() {{
            const options = {{
                show_images: document.getElementById('show-images').checked,
                auto_describe: document.getElementById('auto-describe').checked,
                viewport_mode: document.getElementById('viewport-mode').value,
                vision_model: document.getElementById('vision-model').value,
                text_model: document.getElementById('text-model').value
            }};

            // Save via localStorage for now (will be picked up by abrowser)
            localStorage.setItem('abrowser_options', JSON.stringify(options));

            // Also try to communicate back to abrowser via console
            console.log('ABROWSER_OPTIONS:' + JSON.stringify(options));
        }}

        // Initialize
        checkBackend();
        renderModels();
        updateAutoDescribeVisibility();
    </script>
</body>
</html>"#,
        models_json = models_json,
        models_dir = models_dir.display(),
        desktop_selected = if viewport_mode == "desktop" { "selected" } else { "" },
        mobile_selected = if viewport_mode == "mobile" { "selected" } else { "" },
        show_images_checked = if config.rendering.show_images { "checked" } else { "" },
        auto_describe_checked = if config.rendering.auto_describe { "checked" } else { "" },
    )
}

fn generate_models_json(models: &[ModelInfo], models_dir: &PathBuf) -> String {
    use crate::ai::download::get_download_info;

    let models_with_status: Vec<serde_json::Value> = models
        .iter()
        .map(|m| {
            // Check using download module's filename
            let downloaded = if let Some(info) = get_download_info(&m.id) {
                models_dir.join(&info.filename).exists()
            } else {
                models_dir.join(format!("{}.gguf", m.id)).exists()
            };

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
