#ifndef SELFRIONETTE_STREAM_CADENCE_H
#define SELFRIONETTE_STREAM_CADENCE_H

#include <stdint.h>

#ifndef SELFRIONETTE_QUALITY_STREAM_RATE_HZ
#define SELFRIONETTE_QUALITY_STREAM_RATE_HZ 80
#endif

namespace selfrionette { namespace firmware {

// Maximum sample3 emission attempts, not an ADC conversion-rate guarantee.
// PlatformIO injects this together with the matching firmware version.
constexpr uint32_t kQualityStreamRateHz = SELFRIONETTE_QUALITY_STREAM_RATE_HZ;
static_assert(kQualityStreamRateHz == 20U || kQualityStreamRateHz == 40U ||
              kQualityStreamRateHz == 80U, "quality stream supports 20/40/80 Hz only");
constexpr uint32_t kQualityStreamPeriodMicros = 1000000UL / kQualityStreamRateHz;

// Use elapsed time instead of a signed absolute deadline so even a stall longer
// than 2^31 us stays due. As with micros(), a full unobserved 2^32-us cycle is
// not distinguishable. Save now_us after each attempt, including a queue skip:
// old frames are never accumulated or emitted in a catch-up burst.
inline bool quality_frame_due(uint32_t now_us, uint32_t last_attempt_us) {
  return static_cast<uint32_t>(now_us - last_attempt_us) >=
         kQualityStreamPeriodMicros;
}

}}  // namespace selfrionette::firmware
#endif
