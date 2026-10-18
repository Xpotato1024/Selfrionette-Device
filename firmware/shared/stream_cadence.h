#ifndef SELFRIONETTE_STREAM_CADENCE_H
#define SELFRIONETTE_STREAM_CADENCE_H

#include <stdint.h>

namespace selfrionette { namespace firmware {

// Protocol 3's *maximum emission cadence*, not an ADC acquisition guarantee.
// 40 Hz leaves a conservative margin for the 219-byte worst-case ASCII frame;
// do not claim this means 40 fresh sensor conversions per second.
constexpr uint32_t kQualityStreamRateHz = 40UL;
constexpr uint32_t kQualityStreamPeriodMicros = 1000000UL / kQualityStreamRateHz;
static_assert(kQualityStreamPeriodMicros == 25000UL, "quality cadence must be 25ms");

// Relative deadlines remain valid for deadlines less than 2^31 us ahead.
inline bool deadline_reached(uint32_t now_us, uint32_t deadline_us) {
  return static_cast<int32_t>(now_us - deadline_us) >= 0;
}
inline uint32_t next_deadline(uint32_t now_us) {
  // Do not catch up by emitting multiple stale frames after a long stall.
  return now_us + kQualityStreamPeriodMicros;
}

}} // namespace selfrionette::firmware
#endif
