// Compile-only ATmega32U4 coverage; no device or EEPROM access.
#include "../shared/firmware_protocol_v3.h"
#include "../shared/stream_cadence.h"

using selfrionette::protocol3::Command;
using selfrionette::protocol3::ParseStatus;
using selfrionette::protocol3::SessionStart;
using selfrionette::protocol3::StreamSession;
using selfrionette::protocol3::TareWindow;

bool compile_firmware_protocol_v3(char* payload, Command& command,
                                 TareWindow& tare,
                                 int32_t (&zero)[selfrionette::protocol3::kChannelCount]) {
  const ParseStatus parsed = selfrionette::protocol3::parse_command(payload, command);
  const bool sample_accepted = tare.add(0, -8388607L,
                                        selfrionette::protocol3::kFresh);
  const uint8_t flags = selfrionette::protocol3::flags_for(
      true, true, false, -8388608L, false);
  StreamSession session;
  const SessionStart started = session.start(1);
  uint32_t sequence = 0;
  const bool sequence_advanced = session.advanceSequence(sequence);
  const bool paused = session.stop(1);
  const SessionStart resumed = session.start(1);
  (void)zero;
  return parsed == ParseStatus::Valid && sample_accepted &&
         selfrionette::firmware::quality_frame_due(
             selfrionette::firmware::kQualityStreamPeriodMicros, 0) &&
         started == SessionStart::NewSession && sequence_advanced && sequence == 1 &&
         paused && resumed == SessionStart::Resumed && session.sequence() == 1 &&
         flags == (selfrionette::protocol3::kFresh |
                   selfrionette::protocol3::kSaturated);
}
