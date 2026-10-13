// Compile-only AVR instantiation: declarations below are not a hardware adapter.
#include "../shared/device_settings.h"
struct CompileStore {
  uint8_t read(uint16_t);
  bool update(uint16_t, uint8_t);
};
selfrionette::settings::SaveStatus compile_journal(CompileStore& store, const uint8_t (&id)[16], const selfrionette::settings::Settings& settings, uint32_t expected) {
  return selfrionette::settings::save(store, id, settings, expected);
}
bool compile_orientation(const selfrionette::settings::Loaded& loaded, uint8_t ch, int32_t value, int32_t zero, int32_t& out) {
  return selfrionette::settings::orient_count(loaded, ch, value, zero, out);
}
