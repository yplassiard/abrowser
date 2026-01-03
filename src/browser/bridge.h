#ifndef ABROWSER_SRC_BROWSER_BRIDGE_H_
#define ABROWSER_SRC_BROWSER_BRIDGE_H_

#include "abrowser/src/browser/export.h"

namespace abrowser {

class Renderer;

// Output mode for accessibility rendering
enum class OutputMode {
  Text = 0,         // Lynx-style text output
  ScreenReader = 1, // Screen reader optimized
  Braille = 2,      // Braille display output
};

// Static bridge for global configuration
class ABROWSER_BRIDGE_EXPORT Bridge {
 public:
  static OutputMode GetOutputMode();
  static int GetBrailleCells();
  static bool IsDebugEnabled();

 private:
  friend class Renderer;

  static void Configure(OutputMode mode, int braille_cells, bool debug);
};

}  // namespace abrowser

#endif  // ABROWSER_SRC_BROWSER_BRIDGE_H_
