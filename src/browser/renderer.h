#ifndef ABROWSER_SRC_BROWSER_RENDERER_H_
#define ABROWSER_SRC_BROWSER_RENDERER_H_

#include <cstdint>
#include <memory>
#include <string>
#include <vector>

#include "abrowser/src/browser/export.h"
#include "ui/gfx/geometry/size.h"

extern "C" {

struct abrowser_bridge;

// Browser delegate - callbacks from Rust to C++
struct abrowser_browser_delegate {
  void (*shutdown)();
  void (*refresh)();
  void (*go_to)(const char* url);
  void (*go_back)();
  void (*go_forward)();
  void (*scroll)(int delta);
  void (*key_press)(char key);
  void (*focus_node)(int node_id);
  void (*activate_node)(int node_id);
  void (*post_task)(void (*fn)(void*), void* data);
};

}  // extern "C"

namespace abrowser {

// Accessibility node for passing to Rust
struct ABROWSER_RENDERER_EXPORT AXNodeData {
  int32_t id;
  int32_t role;  // Maps to ax::mojom::Role
  std::string name;
  std::string description;
  std::string value;
  std::string url;
  uint8_t level;  // Heading level
  bool focusable;
  bool focused;
  int32_t parent_id;
  std::vector<int32_t> child_ids;
};

// Main renderer interface - communicates with Rust
class ABROWSER_RENDERER_EXPORT Renderer {
 public:
  // Initialize the renderer (called early in main)
  static void Main();

  // Get the global renderer instance
  static Renderer* GetCurrent();

  // Get terminal size
  gfx::Size GetSize();

  // Start the renderer
  void Start();

  // Resize the renderer
  gfx::Size Resize();

  // Listen for input events (blocking, run in dedicated thread)
  void Listen(const struct abrowser_browser_delegate* delegate);

  // Navigation updates
  void PushNav(const std::string& url, bool can_go_back, bool can_go_forward);
  void SetTitle(const std::string& title);

  // Accessibility tree updates
  void UpdateAccessibilityTree(const std::vector<AXNodeData>& nodes,
                               int32_t root_id);
  void SetFocus(int32_t node_id);

  // Announce text for screen reader
  void Announce(const std::string& text);

 private:
  explicit Renderer(struct abrowser_bridge* ptr);

  struct abrowser_bridge* ptr_;
};

}  // namespace abrowser

#endif  // ABROWSER_SRC_BROWSER_RENDERER_H_
