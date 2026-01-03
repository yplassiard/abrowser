#include "abrowser/src/browser/bridge.h"

namespace {

abrowser::OutputMode output_mode_ = abrowser::OutputMode::Text;
int braille_cells_ = 40;
bool debug_enabled_ = false;

}  // namespace

namespace abrowser {

OutputMode Bridge::GetOutputMode() {
  return output_mode_;
}

int Bridge::GetBrailleCells() {
  return braille_cells_;
}

bool Bridge::IsDebugEnabled() {
  return debug_enabled_;
}

void Bridge::Configure(OutputMode mode, int braille_cells, bool debug) {
  output_mode_ = mode;
  braille_cells_ = braille_cells;
  debug_enabled_ = debug;
}

}  // namespace abrowser
