#include "abrowser/src/browser/renderer.h"

#include <iostream>
#include <memory>

#include "abrowser/src/browser/bridge.h"

extern "C" {

// C structs matching Rust FFI
struct abrowser_size {
  unsigned int width;
  unsigned int height;
};

struct abrowser_ax_node {
  int32_t id;
  int32_t role;
  const char* name;
  const char* description;
  const char* value;
  const char* url;
  uint8_t level;
  bool focusable;
  bool focused;
  int32_t parent_id;
  const int32_t* child_ids;
  uint32_t child_count;
};

// Rust FFI functions
void abrowser_main();
int abrowser_get_output_mode(struct abrowser_bridge* bridge);
int abrowser_get_braille_cells(struct abrowser_bridge* bridge);
bool abrowser_get_debug(struct abrowser_bridge* bridge);

struct abrowser_bridge* abrowser_bridge_create();
void abrowser_bridge_destroy(struct abrowser_bridge* bridge);
void abrowser_bridge_start(struct abrowser_bridge* bridge);
struct abrowser_size abrowser_bridge_get_size(struct abrowser_bridge* bridge);
void abrowser_bridge_resize(struct abrowser_bridge* bridge);
void abrowser_bridge_listen(struct abrowser_bridge* bridge,
                            const struct abrowser_browser_delegate* delegate);

void abrowser_push_nav(struct abrowser_bridge* bridge,
                       const char* url,
                       bool can_go_back,
                       bool can_go_forward);
void abrowser_set_title(struct abrowser_bridge* bridge, const char* title);

void abrowser_update_tree(struct abrowser_bridge* bridge,
                          const struct abrowser_ax_node* nodes,
                          uint32_t node_count,
                          int32_t root_id);
void abrowser_set_focus(struct abrowser_bridge* bridge, int32_t node_id);
void abrowser_announce(struct abrowser_bridge* bridge, const char* text);

}  // extern "C"

namespace abrowser {

namespace {
static std::unique_ptr<Renderer> g_instance;
}  // namespace

Renderer::Renderer(struct abrowser_bridge* ptr) : ptr_(ptr) {}

void Renderer::Main() {
  abrowser_main();
}

Renderer* Renderer::GetCurrent() {
  if (!g_instance) {
    auto* bridge = abrowser_bridge_create();
    g_instance = std::unique_ptr<Renderer>(new Renderer(bridge));

    // Configure the static bridge
    OutputMode mode = static_cast<OutputMode>(abrowser_get_output_mode(bridge));
    int braille_cells = abrowser_get_braille_cells(bridge);
    bool debug = abrowser_get_debug(bridge);
    Bridge::Configure(mode, braille_cells, debug);
  }
  return g_instance.get();
}

gfx::Size Renderer::GetSize() {
  auto size = abrowser_bridge_get_size(ptr_);
  return gfx::Size(size.width, size.height);
}

void Renderer::Start() {
  abrowser_bridge_start(ptr_);
}

gfx::Size Renderer::Resize() {
  abrowser_bridge_resize(ptr_);
  return GetSize();
}

void Renderer::Listen(const struct abrowser_browser_delegate* delegate) {
  abrowser_bridge_listen(ptr_, delegate);
}

void Renderer::PushNav(const std::string& url,
                       bool can_go_back,
                       bool can_go_forward) {
  if (url.empty()) {
    return;
  }
  abrowser_push_nav(ptr_, url.c_str(), can_go_back, can_go_forward);
}

void Renderer::SetTitle(const std::string& title) {
  if (title.empty()) {
    return;
  }
  abrowser_set_title(ptr_, title.c_str());
}

void Renderer::UpdateAccessibilityTree(const std::vector<AXNodeData>& nodes,
                                       int32_t root_id) {
  if (nodes.empty()) {
    return;
  }

  // Allocate C-compatible node array
  std::vector<abrowser_ax_node> c_nodes;
  c_nodes.reserve(nodes.size());

  for (const auto& node : nodes) {
    abrowser_ax_node c_node;
    c_node.id = node.id;
    c_node.role = node.role;
    c_node.name = node.name.c_str();
    c_node.description = node.description.c_str();
    c_node.value = node.value.c_str();
    c_node.url = node.url.empty() ? nullptr : node.url.c_str();
    c_node.level = node.level;
    c_node.focusable = node.focusable;
    c_node.focused = node.focused;
    c_node.parent_id = node.parent_id;
    c_node.child_ids = node.child_ids.empty() ? nullptr : node.child_ids.data();
    c_node.child_count = static_cast<uint32_t>(node.child_ids.size());
    c_nodes.push_back(c_node);
  }

  abrowser_update_tree(ptr_, c_nodes.data(),
                       static_cast<uint32_t>(c_nodes.size()), root_id);
}

void Renderer::SetFocus(int32_t node_id) {
  abrowser_set_focus(ptr_, node_id);
}

void Renderer::Announce(const std::string& text) {
  if (text.empty()) {
    return;
  }
  abrowser_announce(ptr_, text.c_str());
}

}  // namespace abrowser
