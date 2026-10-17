#ifndef SELFRIONETTE_FIRMWARE_PROTOCOL_V3_H
#define SELFRIONETTE_FIRMWARE_PROTOCOL_V3_H

#include <stddef.h>
#include <stdint.h>
#include <string.h>

namespace selfrionette { namespace protocol3 {

constexpr uint8_t kChannelCount = 7;
constexpr uint16_t kCommandPayloadMax = 87;
constexpr uint16_t kMaxValidCommandPayload = kCommandPayloadMax;
constexpr uint16_t kFramePayloadMax = 511;
constexpr uint16_t kMaxSample3LineBytes = 219;
constexpr uint8_t kTareSamplesPerChannel = 8;
constexpr uint16_t kTareMaxSpread = 2000;
constexpr uint32_t kTareTimeoutMs = 2000;
constexpr uint32_t kFreshTimeoutMs = 50;

enum QualityFlag : uint8_t {
  kFresh = 1,
  kTimeout = 2,
  kSaturated = 4,
  kHeld = 8,
  kRejected = 16
};

enum class CommandKind : uint8_t { None, Info, Stream, Settings, Tare };
enum class ParseStatus : uint8_t { NotV3, Valid, InvalidRequest, Malformed };
enum class SessionStart : uint8_t { Invalid, AlreadyActive, Resumed, NewSession };

struct Command {
  CommandKind kind;
  uint32_t request;
  uint32_t session;
  uint32_t expected_generation;
  uint32_t assembly;
  uint8_t identity[16];
  uint8_t known;
  uint8_t negative;
  bool enabled;
  bool has_request;
};

class StreamSession {
 public:
  StreamSession() : challenge_(0), sequence_(0), active_(false) {}

  SessionStart start(uint32_t challenge) {
    if (challenge == 0) return SessionStart::Invalid;
    if (challenge_ != challenge) {
      challenge_ = challenge;
      sequence_ = 0;
      active_ = true;
      return SessionStart::NewSession;
    }
    if (active_) return SessionStart::AlreadyActive;
    active_ = true;
    return SessionStart::Resumed;
  }

  bool stop(uint32_t challenge) {
    if (challenge_ == 0 || challenge_ != challenge) return false;
    active_ = false;
    return true;
  }

  void deactivate() { active_ = false; }
  bool active() const { return active_; }
  uint32_t challenge() const { return challenge_; }
  uint32_t sequence() const { return sequence_; }

  bool advanceSequence(uint32_t& output) {
    if (sequence_ == UINT32_MAX) return false;
    output = ++sequence_;
    return true;
  }

 private:
  uint32_t challenge_;
  uint32_t sequence_;
  bool active_;
};

inline bool equals(const char* value, const char* expected) {
  return strcmp(value, expected) == 0;
}

inline bool is_command_name(const char* value) {
  return equals(value, "info3") || equals(value, "stream3") ||
         equals(value, "settings3") || equals(value, "tare3");
}

inline bool is_v3_payload(const char* payload) {
  const char* end = strchr(payload, ',');
  const size_t length = end == NULL ? strlen(payload) : static_cast<size_t>(end - payload);
  return (length == 5 && memcmp(payload, "info3", 5) == 0) ||
         (length == 7 && memcmp(payload, "stream3", 7) == 0) ||
         (length == 9 && memcmp(payload, "settings3", 9) == 0) ||
         (length == 5 && memcmp(payload, "tare3", 5) == 0);
}

inline bool parse_u32(const char* text, uint32_t& output) {
  if (text[0] == '\0') return false;
  uint32_t value = 0;
  for (const char* cursor = text; *cursor != '\0'; ++cursor) {
    if (*cursor < '0' || *cursor > '9') return false;
    const uint8_t digit = static_cast<uint8_t>(*cursor - '0');
    if (value > (UINT32_MAX - digit) / 10U) return false;
    value = value * 10U + digit;
  }
  output = value;
  return true;
}

inline int8_t hex_nibble(char value) {
  if (value >= '0' && value <= '9') return static_cast<int8_t>(value - '0');
  if (value >= 'a' && value <= 'f') return static_cast<int8_t>(10 + value - 'a');
  return -1;
}

inline bool parse_device_id(const char* text, uint8_t (&output)[16]) {
  if (strlen(text) != 36 || memcmp(text, "srn-", 4) != 0) return false;
  uint8_t combined = 0;
  for (uint8_t i = 0; i < 16; ++i) {
    const int8_t high = hex_nibble(text[4 + 2 * i]);
    const int8_t low = hex_nibble(text[5 + 2 * i]);
    if (high < 0 || low < 0) return false;
    output[i] = static_cast<uint8_t>((high << 4) | low);
    combined |= output[i];
  }
  return combined != 0;
}

inline ParseStatus parse_command(char* payload, Command& output) {
  char* fields[7];
  uint8_t field_count = 1;
  fields[0] = payload;
  for (char* cursor = payload; *cursor != '\0'; ++cursor) {
    if (*cursor == ',') {
      *cursor = '\0';
      if (field_count >= sizeof(fields) / sizeof(fields[0])) return ParseStatus::Malformed;
      fields[field_count++] = cursor + 1;
    }
  }

  if (!is_command_name(fields[0])) return ParseStatus::NotV3;
  output = Command();
  if (field_count < 2 || !parse_u32(fields[1], output.request) || output.request == 0) {
    return ParseStatus::InvalidRequest;
  }
  output.has_request = true;

  if (equals(fields[0], "info3")) {
    if (field_count != 2) return ParseStatus::Malformed;
    output.kind = CommandKind::Info;
    return ParseStatus::Valid;
  }

  if (equals(fields[0], "stream3")) {
    if (field_count != 4 || !parse_u32(fields[2], output.session) ||
        output.session == 0 || (!equals(fields[3], "0") && !equals(fields[3], "1"))) {
      return ParseStatus::Malformed;
    }
    output.kind = CommandKind::Stream;
    output.enabled = equals(fields[3], "1");
    return ParseStatus::Valid;
  }

  if (equals(fields[0], "settings3")) {
    uint32_t known = 0;
    uint32_t negative = 0;
    if (field_count != 7 || !parse_device_id(fields[2], output.identity) ||
        !parse_u32(fields[3], output.expected_generation) ||
        !parse_u32(fields[4], output.assembly) || output.assembly == 0 ||
        !parse_u32(fields[5], known) || known > 0x7fU ||
        !parse_u32(fields[6], negative) || negative > 0x7fU ||
        (negative & ~known) != 0) {
      return ParseStatus::Malformed;
    }
    output.kind = CommandKind::Settings;
    output.known = static_cast<uint8_t>(known);
    output.negative = static_cast<uint8_t>(negative);
    return ParseStatus::Valid;
  }

  if (field_count != 4 || !parse_device_id(fields[2], output.identity) ||
      !parse_u32(fields[3], output.expected_generation)) {
    return ParseStatus::Malformed;
  }
  output.kind = CommandKind::Tare;
  return ParseStatus::Valid;
}

inline bool is_saturated(int32_t raw) {
  return raw == -8388608L || raw == 8388607L;
}

inline uint8_t flags_for(bool has_raw, bool fresh, bool timed_out,
                         int32_t raw, bool rejected) {
  if (!has_raw) return kTimeout;
  if (fresh) {
    uint8_t flags = kFresh;
    if (is_saturated(raw)) flags |= kSaturated;
    if (rejected) flags |= kRejected;
    return flags;
  }
  uint8_t flags = static_cast<uint8_t>(kHeld | (timed_out ? kTimeout : 0));
  if (is_saturated(raw)) flags |= kSaturated;
  if (rejected) flags |= kRejected;
  return flags;
}

inline bool eligible_fresh(uint8_t flags) { return flags == kFresh; }

inline bool deadline_elapsed(uint32_t now_ms, uint32_t started_ms, uint32_t timeout_ms) {
  return static_cast<uint32_t>(now_ms - started_ms) >= timeout_ms;
}

enum class TareAdmission : uint8_t { Added, Rejected, TimedOut };
enum class TareCompletion : uint8_t { Incomplete, Complete, Unstable, TimedOut };

class TareWindow {
 public:
  TareWindow() { reset(); }

  void reset() {
    memset(count_, 0, sizeof(count_));
    memset(sum_, 0, sizeof(sum_));
    memset(min_, 0, sizeof(min_));
    memset(max_, 0, sizeof(max_));
  }

  bool add(uint8_t channel, int32_t raw, uint8_t flags) {
    if (channel >= kChannelCount || count_[channel] >= kTareSamplesPerChannel ||
        flags != kFresh || is_saturated(raw) || raw < -8388608L || raw > 8388607L) {
      return false;
    }
    if (count_[channel] == 0) {
      min_[channel] = raw;
      max_[channel] = raw;
    } else {
      if (raw < min_[channel]) min_[channel] = raw;
      if (raw > max_[channel]) max_[channel] = raw;
    }
    sum_[channel] += raw;
    ++count_[channel];
    return true;
  }

  TareAdmission add_before_deadline(uint32_t now_ms, uint32_t started_ms,
                                   uint8_t channel, int32_t raw,
                                   uint8_t flags) {
    if (deadline_elapsed(now_ms, started_ms, kTareTimeoutMs)) {
      return TareAdmission::TimedOut;
    }
    return add(channel, raw, flags) ? TareAdmission::Added
                                    : TareAdmission::Rejected;
  }

  bool complete() const {
    for (uint8_t channel = 0; channel < kChannelCount; ++channel) {
      if (count_[channel] != kTareSamplesPerChannel) return false;
    }
    return true;
  }

  bool finish(int32_t (&zero)[kChannelCount]) const {
    if (!complete()) return false;
    int32_t candidate[kChannelCount];
    for (uint8_t channel = 0; channel < kChannelCount; ++channel) {
      if (static_cast<uint32_t>(max_[channel] - min_[channel]) > kTareMaxSpread) return false;
      candidate[channel] = sum_[channel] / kTareSamplesPerChannel;
    }
    memcpy(zero, candidate, sizeof(candidate));
    return true;
  }

  TareCompletion finish_before_deadline(uint32_t now_ms, uint32_t started_ms,
                                        int32_t (&zero)[kChannelCount]) const {
    if (deadline_elapsed(now_ms, started_ms, kTareTimeoutMs)) {
      return TareCompletion::TimedOut;
    }
    if (!complete()) return TareCompletion::Incomplete;
    return finish(zero) ? TareCompletion::Complete : TareCompletion::Unstable;
  }

  uint8_t count(uint8_t channel) const {
    return channel < kChannelCount ? count_[channel] : 0;
  }

 private:
  uint8_t count_[kChannelCount];
  int32_t sum_[kChannelCount];
  int32_t min_[kChannelCount];
  int32_t max_[kChannelCount];
};

}}  // namespace selfrionette::protocol3

#endif
