#include <Arduino.h>
#include <EEPROM.h>
#include <math.h>
#include <string.h>

#include "HX711.h"
#include "../../shared/buffered_line_queue.h"
#include "../../shared/device_settings.h"
#include "../../shared/firmware_protocol_v3.h"
#include "../../shared/stream_cadence.h"

namespace {
#ifndef SELFRIONETTE_FIRMWARE_VERSION
#error "PlatformIO must inject the independently versioned Managed firmware release"
#endif

#ifndef SELFRIONETTE_SOURCE_SHA
#error "PlatformIO must inject SELFRIONETTE_SOURCE_SHA"
#endif
static_assert(sizeof(SELFRIONETTE_SOURCE_SHA) == 41,
              "source SHA must contain exactly 40 characters");

constexpr uint8_t kProtocolMajorVersion = 2;
constexpr uint8_t kChannelCount = 7;
constexpr uint8_t kAllChannelBits = (1U << kChannelCount) - 1U;
constexpr unsigned long kSerialBaudRate = 115200UL;
constexpr unsigned long kSampleRateHz = 80UL;
constexpr unsigned long kSamplePeriodMicros = 1000000UL / kSampleRateHz;
static_assert(kSamplePeriodMicros == 12500UL, "v2 compatibility must stay at 80 Hz");
constexpr uint32_t kV3SamplePeriodMicros = selfrionette::firmware::kQualityStreamPeriodMicros;
constexpr unsigned long kReadyTimeoutMs = 25UL;
constexpr unsigned long kReadyPollDelayMs = 1UL;
constexpr uint8_t kSensorActivationReads = 10;
constexpr uint8_t kCalibrationWarmupReads = 5;
constexpr uint8_t kCalibrationBatchCount = 3;
constexpr uint8_t kCalibrationBatchSampleCount = 17;
constexpr unsigned long kCalibrationReadDelayMicros = 1000UL;
constexpr unsigned long kCalibrationBatchSettleDelayMs = 20UL;
constexpr double kMaxChangeThreshold = 100000.0;
constexpr double kCalibrationBatchSpreadThreshold = 2000.0;

constexpr uint8_t kLoadcellDoutPins[kChannelCount] = {4, 6, 8, 10, 19, 3, 14};
constexpr uint8_t kLoadcellSckPins[kChannelCount] = {5, 7, 9, 18, 20, 2, 15};
constexpr uint16_t kOutputBufferCapacity = 256;

class BufferedSerialOutput : public Print {
 public:
  bool beginLine(uint16_t reserve = 0) { return queue_.beginLine(reserve); }

  bool endLine() {
    const bool accepted = queue_.endLine();
    if (!accepted) return false;
    drain();
    return true;
  }

  bool busy() const { return queue_.busy(); }

  bool canQueue(uint16_t required) { return queue_.canQueue(required); }

  void drain() { queue_.drain(Serial); }

  size_t write(uint8_t value) override {
    return queue_.write(value);
  }

 private:
  selfrionette::firmware::BufferedLineQueue<kOutputBufferCapacity> queue_;
};

BufferedSerialOutput g_output;
constexpr uint16_t kProtocolReplyReserveBytes = 37;
constexpr uint16_t kInfo3MaxLineBytes = 200;
constexpr uint16_t kCommandReplyReserveBytes =
    kInfo3MaxLineBytes + kProtocolReplyReserveBytes;
constexpr uint16_t kV3MaxSampleLineBytes = selfrionette::protocol3::kMaxSample3LineBytes;
constexpr uint16_t kSampleQueueReserveBytes =
    kV3MaxSampleLineBytes + kProtocolReplyReserveBytes;
static_assert(kCommandReplyReserveBytes <= kOutputBufferCapacity,
              "output buffer must reserve the largest v3 command reply and tare result");
static_assert(kSampleQueueReserveBytes <= kOutputBufferCapacity,
              "output buffer must reserve a full sample and asynchronous tare result");

constexpr uint16_t kCommandBufferSize =
    selfrionette::protocol3::kMaxValidCommandPayload + 1;
char g_command_buffer[kCommandBufferSize];
uint8_t g_command_length = 0;
bool g_command_overflow = false;
bool g_command_ready = false;

constexpr int kIdentityBaseAddress = 0;
constexpr uint8_t kIdentityMagic[4] = {'S', 'R', 'N', '2'};
constexpr uint8_t kIdentitySchemaVersion = 1;
constexpr uint8_t kIdentitySize = 16;
constexpr int kIdentityMagicOffset = 0;
constexpr int kIdentitySchemaOffset = 4;
constexpr int kIdentityPayloadOffset = 5;
constexpr int kIdentityCrcOffset = 21;
constexpr int kIdentityRecordSize = 23;

uint8_t g_device_id[kIdentitySize] = {0};
bool g_device_identity_valid = false;

HX711 g_scales[kChannelCount];
double g_offsets[kChannelCount] = {0, 0, 0, 0, 0, 0, 0};
double g_previous_values[kChannelCount] = {0, 0, 0, 0, 0, 0, 0};
double g_current_values[kChannelCount] = {0, 0, 0, 0, 0, 0, 0};
uint8_t g_legacy_timeout_warned = 0;

using selfrionette::protocol3::Command;
using selfrionette::protocol3::CommandKind;
using selfrionette::protocol3::ParseStatus;
using selfrionette::protocol3::SessionStart;
using selfrionette::protocol3::StreamSession;
using selfrionette::protocol3::TareWindow;
using selfrionette::settings::Loaded;
using selfrionette::settings::Settings;

struct EepromStore {
  uint8_t read(uint16_t address) const { return EEPROM.read(address); }
  bool update(uint16_t address, uint8_t value) {
    EEPROM.update(address, value);
    return EEPROM.read(address) == value;
  }
};

Loaded g_settings = {};
StreamSession g_v3_stream;
uint32_t g_v2_next_frame_us = 0;
uint32_t g_v3_next_frame_us = 0;
int32_t g_v3_raw[kChannelCount] = {0, 0, 0, 0, 0, 0, 0};
uint32_t g_v3_last_read_ms[kChannelCount] = {0, 0, 0, 0, 0, 0, 0};
uint8_t g_v3_has_raw = 0;
uint8_t g_v3_fresh_this_frame = 0;
uint8_t g_v3_sample_rejected = 0;
uint8_t g_v3_rejected_next = 0;
bool g_tare_active = false;
bool g_tare_valid = false;
uint32_t g_tare_epoch = 0;
uint32_t g_tare_request = 0;
uint32_t g_tare_started_ms = 0;
int32_t g_tare_zero[kChannelCount] = {0, 0, 0, 0, 0, 0, 0};
TareWindow g_tare_window;

uint8_t channelBit(uint8_t channel) {
  return static_cast<uint8_t>(1U << channel);
}

bool channelFlag(uint8_t flags, uint8_t channel) {
  return (flags & channelBit(channel)) != 0;
}

void setChannelFlag(uint8_t& flags, uint8_t channel, bool enabled) {
  const uint8_t bit = channelBit(channel);
  if (enabled) flags |= bit;
  else flags &= static_cast<uint8_t>(~bit);
}

void emitStatus(const __FlashStringHelper* token) {
  if (!g_output.beginLine(kProtocolReplyReserveBytes)) return;
  g_output.print(F("status,"));
  g_output.print(token);
  g_output.endLine();
}

void emitStatusChannelValue(const __FlashStringHelper* token, uint8_t channel, long value) {
  if (!g_output.beginLine(kProtocolReplyReserveBytes)) return;
  g_output.print(F("status,"));
  g_output.print(token);
  g_output.print(',');
  g_output.print(channel);
  g_output.print(',');
  g_output.print(value);
  g_output.endLine();
}

void emitWarn(const __FlashStringHelper* token) {
  if (!g_output.beginLine(kProtocolReplyReserveBytes)) return;
  g_output.print(F("warn,"));
  g_output.print(token);
  g_output.endLine();
}

void emitWarnChannel(const __FlashStringHelper* token, uint8_t channel) {
  if (!g_output.beginLine(kProtocolReplyReserveBytes)) return;
  g_output.print(F("warn,"));
  g_output.print(token);
  g_output.print(',');
  g_output.print(channel);
  g_output.endLine();
}

void emitWarnChannelValue(const __FlashStringHelper* token, uint8_t channel, double value) {
  if (!g_output.beginLine(kProtocolReplyReserveBytes)) return;
  g_output.print(F("warn,"));
  g_output.print(token);
  g_output.print(',');
  g_output.print(channel);
  g_output.print(',');
  g_output.print(value);
  g_output.endLine();
}

void printVectorLine(unsigned long timestamp_ms, const double* values) {
  if (!g_output.beginLine(kProtocolReplyReserveBytes)) return;
  g_output.print(F("vector,"));
  g_output.print(timestamp_ms);
  for (uint8_t i = 0; i < kChannelCount; ++i) {
    g_output.print(',');
    g_output.print(values[i]);
  }
  g_output.endLine();
}

uint16_t updateCrc16Ccitt(uint16_t crc, uint8_t value) {
  crc ^= static_cast<uint16_t>(value) << 8;
  for (uint8_t bit = 0; bit < 8; ++bit) {
    if ((crc & 0x8000U) != 0U) {
      crc = static_cast<uint16_t>((crc << 1) ^ 0x1021U);
    } else {
      crc <<= 1;
    }
  }
  return crc;
}

uint16_t identityCrc(const uint8_t* id) {
  uint16_t crc = 0xFFFFU;
  for (uint8_t i = 0; i < 4; ++i) {
    crc = updateCrc16Ccitt(crc, kIdentityMagic[i]);
  }
  crc = updateCrc16Ccitt(crc, kIdentitySchemaVersion);
  for (uint8_t i = 0; i < kIdentitySize; ++i) {
    crc = updateCrc16Ccitt(crc, id[i]);
  }
  return crc;
}

bool isAllZeroIdentity(const uint8_t* id) {
  uint8_t combined = 0;
  for (uint8_t i = 0; i < kIdentitySize; ++i) {
    combined |= id[i];
  }
  return combined == 0;
}

bool loadDeviceIdentity() {
  for (uint8_t i = 0; i < 4; ++i) {
    if (EEPROM.read(kIdentityBaseAddress + kIdentityMagicOffset + i) != kIdentityMagic[i]) {
      return false;
    }
  }

  if (EEPROM.read(kIdentityBaseAddress + kIdentitySchemaOffset) != kIdentitySchemaVersion) {
    return false;
  }

  for (uint8_t i = 0; i < kIdentitySize; ++i) {
    g_device_id[i] = EEPROM.read(kIdentityBaseAddress + kIdentityPayloadOffset + i);
  }

  if (isAllZeroIdentity(g_device_id)) {
    return false;
  }

  const uint16_t stored_crc =
      (static_cast<uint16_t>(EEPROM.read(kIdentityBaseAddress + kIdentityCrcOffset)) << 8) |
      static_cast<uint16_t>(EEPROM.read(kIdentityBaseAddress + kIdentityCrcOffset + 1));

  return stored_crc == identityCrc(g_device_id);
}

void writeDeviceIdentity(const uint8_t* id) {
  // Invalidate first so a power loss cannot leave a partially written record looking valid.
  for (uint8_t i = 0; i < 4; ++i) {
    EEPROM.update(kIdentityBaseAddress + kIdentityMagicOffset + i, 0);
  }

  EEPROM.update(kIdentityBaseAddress + kIdentitySchemaOffset, kIdentitySchemaVersion);
  for (uint8_t i = 0; i < kIdentitySize; ++i) {
    EEPROM.update(kIdentityBaseAddress + kIdentityPayloadOffset + i, id[i]);
  }

  const uint16_t crc = identityCrc(id);
  EEPROM.update(kIdentityBaseAddress + kIdentityCrcOffset, static_cast<uint8_t>(crc >> 8));
  EEPROM.update(kIdentityBaseAddress + kIdentityCrcOffset + 1, static_cast<uint8_t>(crc & 0xFF));

  for (uint8_t i = 0; i < 4; ++i) {
    EEPROM.update(kIdentityBaseAddress + kIdentityMagicOffset + i, kIdentityMagic[i]);
  }
}

char hexDigit(uint8_t value) {
  return value < 10 ? static_cast<char>('0' + value) : static_cast<char>('a' + (value - 10));
}

void printDeviceId(const uint8_t* id) {
  g_output.print(F("srn-"));
  for (uint8_t i = 0; i < kIdentitySize; ++i) {
    g_output.print(hexDigit(static_cast<uint8_t>(id[i] >> 4)));
    g_output.print(hexDigit(static_cast<uint8_t>(id[i] & 0x0F)));
  }
}

void emitDeviceInfo() {
  if (!g_output.beginLine(kProtocolReplyReserveBytes)) return;
  g_output.print(F("device,"));
  g_output.print(kProtocolMajorVersion);
  g_output.print(',');
  g_output.print(F(SELFRIONETTE_FIRMWARE_VERSION));
  g_output.print(',');
  if (g_device_identity_valid) {
    printDeviceId(g_device_id);
  } else {
    g_output.print(F("unprovisioned"));
  }
  g_output.print(',');
  g_output.print(kChannelCount);
  g_output.endLine();
}

void emitError3(uint32_t request, const __FlashStringHelper* token) {
  if (!g_output.beginLine()) return;
  g_output.print(F("error3,"));
  g_output.print(static_cast<unsigned long>(request));
  g_output.print(',');
  g_output.print(token);
  g_output.endLine();
}

void emitOk3(uint32_t request, const __FlashStringHelper* operation, uint32_t value) {
  if (!g_output.beginLine()) return;
  g_output.print(F("ok3,"));
  g_output.print(static_cast<unsigned long>(request));
  g_output.print(',');
  g_output.print(operation);
  g_output.print(',');
  g_output.print(static_cast<unsigned long>(value));
  g_output.endLine();
}

bool settings_available() {
  return g_settings.state == selfrionette::settings::State::Empty ||
         g_settings.state == selfrionette::settings::State::Partial ||
         g_settings.state == selfrionette::settings::State::Usable;
}

uint32_t current_generation() {
  return g_settings.state == selfrionette::settings::State::Empty
             ? 0U
             : g_settings.value.generation;
}

void emitDeviceInfo3(uint32_t request) {
  if (!settings_available()) {
    emitError3(request, F("settings_corrupt"));
    return;
  }

  const Settings* settings = &g_settings.value;
  const uint32_t generation = current_generation();
  const uint32_t assembly = generation == 0 ? 0U : settings->assembly;
  const uint8_t known = generation == 0 ? 0 : settings->known;
  const uint8_t negative = generation == 0 ? 0 : settings->negative;

  if (!g_output.beginLine()) return;
  g_output.print(F("info3,"));
  g_output.print(static_cast<unsigned long>(request));
  g_output.print(',');
  g_output.print(F(SELFRIONETTE_FIRMWARE_VERSION));
  g_output.print(',');
  g_output.print(F(SELFRIONETTE_SOURCE_SHA));
  g_output.print(F(",sparkfun_promicro16,atmega32u4,16000000,"));
  if (g_device_identity_valid) printDeviceId(g_device_id);
  else g_output.print(F("unprovisioned"));
  g_output.print(',');
  g_output.print(static_cast<unsigned long>(generation));
  g_output.print(',');
  g_output.print(static_cast<unsigned long>(assembly));
  g_output.print(',');
  g_output.print(known);
  g_output.print(',');
  g_output.print(negative);
  g_output.print(',');
  g_output.print(static_cast<unsigned long>(g_tare_epoch));
  g_output.print(',');
  g_output.print(g_tare_valid ? 1 : 0);
  g_output.endLine();
}

int8_t parseLowerHexNibble(char value) {
  if (value >= '0' && value <= '9') {
    return static_cast<int8_t>(value - '0');
  }
  if (value >= 'a' && value <= 'f') {
    return static_cast<int8_t>(10 + value - 'a');
  }
  return -1;
}

bool parseDeviceId(const char* text, uint8_t* output) {
  constexpr uint8_t kPrefixLength = 4;
  constexpr uint8_t kHexLength = 32;
  constexpr uint8_t kTextLength = kPrefixLength + kHexLength;

  if (strlen(text) != kTextLength || strncmp(text, "srn-", kPrefixLength) != 0) {
    return false;
  }

  for (uint8_t i = 0; i < kIdentitySize; ++i) {
    const int8_t high = parseLowerHexNibble(text[kPrefixLength + i * 2]);
    const int8_t low = parseLowerHexNibble(text[kPrefixLength + i * 2 + 1]);
    if (high < 0 || low < 0) {
      return false;
    }
    output[i] = static_cast<uint8_t>((high << 4) | low);
  }

  return !isAllZeroIdentity(output);
}

void invalidateTare() {
  g_tare_valid = false;
  memset(g_tare_zero, 0, sizeof(g_tare_zero));
}

void cancelTare(const __FlashStringHelper* reason) {
  if (!g_tare_active) return;
  emitError3(g_tare_request, reason);
  g_tare_active = false;
  g_tare_window.reset();
}

void reloadSettings() {
  EepromStore store;
  g_settings = selfrionette::settings::load(store, g_device_id);
  if (!g_device_identity_valid && g_settings.state != selfrionette::settings::State::Empty) {
    g_settings.state = selfrionette::settings::State::Corrupt;
  }
}

bool requestIdentityMatches(const Command& command) {
  return g_device_identity_valid &&
         memcmp(command.identity, g_device_id, kIdentitySize) == 0;
}

void beginTare(const Command& command) {
  if (!g_device_identity_valid || !requestIdentityMatches(command)) {
    emitError3(command.request, F("device_mismatch"));
    return;
  }
  if (!settings_available()) {
    emitError3(command.request, F("settings_corrupt"));
    return;
  }
  if (command.expected_generation != current_generation()) {
    emitError3(command.request, F("stale_generation"));
    return;
  }
  if (g_tare_active) {
    emitError3(command.request, F("busy"));
    return;
  }
  if (g_tare_epoch == UINT32_MAX) {
    emitError3(command.request, F("tare_epoch_exhausted"));
    return;
  }

  invalidateTare();
  g_tare_request = command.request;
  g_tare_started_ms = millis();
  g_tare_window.reset();
  g_tare_active = true;
}

void failTare(const __FlashStringHelper* reason) {
  if (!g_tare_active) return;
  const uint32_t request = g_tare_request;
  g_tare_active = false;
  g_tare_window.reset();
  invalidateTare();
  g_v3_rejected_next = kAllChannelBits;
  emitError3(request, reason);
}

void finishTareIfReady(uint32_t now_ms) {
  if (!g_tare_active || !g_tare_window.complete()) return;
  int32_t candidate[kChannelCount];
  const selfrionette::protocol3::TareCompletion completion =
      g_tare_window.finish_before_deadline(now_ms, g_tare_started_ms, candidate);
  if (completion == selfrionette::protocol3::TareCompletion::TimedOut) {
    failTare(F("tare_timeout"));
    return;
  }
  if (completion == selfrionette::protocol3::TareCompletion::Unstable) {
    failTare(F("tare_quality"));
    return;
  }
  if (completion != selfrionette::protocol3::TareCompletion::Complete) return;

  memcpy(g_tare_zero, candidate, sizeof(g_tare_zero));
  ++g_tare_epoch;
  g_tare_valid = true;
  g_tare_active = false;
  g_tare_window.reset();
  emitOk3(g_tare_request, F("tare"), g_tare_epoch);
}

void observeRawReading(uint8_t channel, long raw, uint32_t now_ms) {
  const double legacy_reading = -static_cast<double>(raw);
  const double adjusted = legacy_reading - g_offsets[channel];
  if (fabs(adjusted - g_previous_values[channel]) > kMaxChangeThreshold) {
    g_current_values[channel] = g_previous_values[channel];
    emitWarnChannelValue(F("spike"), channel, g_current_values[channel]);
  } else {
    g_current_values[channel] = adjusted;
  }
  g_previous_values[channel] = g_current_values[channel];
  setChannelFlag(g_legacy_timeout_warned, channel, false);

  g_v3_raw[channel] = static_cast<int32_t>(raw);
  g_v3_last_read_ms[channel] = now_ms;
  setChannelFlag(g_v3_has_raw, channel, true);
  setChannelFlag(g_v3_fresh_this_frame, channel, true);
  setChannelFlag(g_v3_sample_rejected, channel,
                 channelFlag(g_v3_rejected_next, channel));
  setChannelFlag(g_v3_rejected_next, channel, false);

  if (g_tare_active) {
    const uint8_t flags = selfrionette::protocol3::flags_for(
        true, true, false, static_cast<int32_t>(raw), false);
    const selfrionette::protocol3::TareAdmission admission =
        g_tare_window.add_before_deadline(
            now_ms, g_tare_started_ms, channel, static_cast<int32_t>(raw), flags);
    if (admission == selfrionette::protocol3::TareAdmission::TimedOut) {
      failTare(F("tare_timeout"));
    } else if (admission == selfrionette::protocol3::TareAdmission::Rejected) {
      setChannelFlag(g_v3_rejected_next, channel, true);
    }
    if (g_tare_active) finishTareIfReady(millis());
  }
}

void updateSensors() {
  const uint32_t now_ms = millis();
  for (uint8_t channel = 0; channel < kChannelCount; ++channel) {
    long raw = 0;
    if (g_scales[channel].read_raw_if_ready(&raw)) {
      observeRawReading(channel, raw, millis());
    } else if (!channelFlag(g_legacy_timeout_warned, channel) &&
               selfrionette::protocol3::deadline_elapsed(
                   now_ms, g_v3_last_read_ms[channel], kReadyTimeoutMs)) {
      emitWarnChannel(F("ready_timeout"), channel);
      setChannelFlag(g_legacy_timeout_warned, channel, true);
    }
  }

  if (g_tare_active && selfrionette::protocol3::deadline_elapsed(
                           millis(), g_tare_started_ms,
                           selfrionette::protocol3::kTareTimeoutMs)) {
    failTare(F("tare_timeout"));
  }
}

bool emitSample3(uint32_t now_ms) {
  if (!g_v3_stream.active()) return false;
  if (g_v3_stream.sequence() == UINT32_MAX) {
    g_v3_stream.deactivate();
    return false;
  }
  if (!g_output.canQueue(kSampleQueueReserveBytes)) return false;
  uint32_t sequence = 0;
  if (!g_v3_stream.advanceSequence(sequence)) {
    g_v3_stream.deactivate();
    return false;
  }
  if (!g_output.beginLine()) return false;

  g_output.print(F("sample3,"));
  g_output.print(static_cast<unsigned long>(g_v3_stream.challenge()));
  g_output.print(',');
  g_output.print(static_cast<unsigned long>(sequence));
  g_output.print(',');
  g_output.print(static_cast<unsigned long>(now_ms));
  g_output.print(',');
  g_output.print(static_cast<unsigned long>(g_tare_epoch));
  g_output.print(',');
  g_output.print(static_cast<unsigned long>(current_generation()));
  g_output.print(',');
  g_output.print(g_tare_valid ? 1 : 0);

  uint8_t flags[kChannelCount];
  for (uint8_t channel = 0; channel < kChannelCount; ++channel) {
    const bool timed_out = selfrionette::protocol3::deadline_elapsed(
        now_ms, g_v3_last_read_ms[channel],
        selfrionette::protocol3::kFreshTimeoutMs);
    const bool fresh = channelFlag(g_v3_fresh_this_frame, channel) && !timed_out;
    flags[channel] = selfrionette::protocol3::flags_for(
        channelFlag(g_v3_has_raw, channel), fresh, timed_out, g_v3_raw[channel],
        channelFlag(g_v3_sample_rejected, channel));
    g_output.print(',');
    if (channelFlag(g_v3_has_raw, channel))
      g_output.print(static_cast<long>(g_v3_raw[channel]));
    else g_output.print('?');
  }

  for (uint8_t channel = 0; channel < kChannelCount; ++channel) {
    int32_t oriented = 0;
    const bool usable = flags[channel] == selfrionette::protocol3::kFresh &&
        g_tare_valid && selfrionette::settings::orient_count(
            g_settings, channel, g_v3_raw[channel], g_tare_zero[channel], oriented);
    g_output.print(',');
    if (usable) g_output.print(static_cast<long>(oriented));
    else g_output.print('?');
  }

  for (uint8_t channel = 0; channel < kChannelCount; ++channel) {
    g_output.print(',');
    g_output.print(flags[channel]);
  }
  g_v3_fresh_this_frame = 0;
  g_v3_sample_rejected = 0;
  return g_output.endLine();
}

bool waitForReady(uint8_t channel) {
  return g_scales[channel].wait_ready_timeout(kReadyTimeoutMs, kReadyPollDelayMs);
}

bool readSignedReading(uint8_t channel, long* value) {
  long raw = 0;
  if (!g_scales[channel].read_raw_if_ready(&raw)) return false;
  *value = -raw;
  return true;
}

bool collectReadings(uint8_t channel, uint8_t sampleCount, long* samples, const __FlashStringHelper* timeoutReason) {
  if (sampleCount == 0) {
    return false;
  }

  for (uint8_t i = 0; i < sampleCount; ++i) {
    if (!waitForReady(channel)) {
      emitWarnChannel(timeoutReason, channel);
      return false;
    }

    if (!readSignedReading(channel, &samples[i])) {
      emitWarnChannel(timeoutReason, channel);
      return false;
    }
    delayMicroseconds(kCalibrationReadDelayMicros);
  }

  return true;
}

double trimmedMean(const long* samples, uint8_t sampleCount) {
  if (sampleCount == 0) {
    return 0.0;
  }

  long sum = 0;
  long min_value = samples[0];
  long max_value = samples[0];
  for (uint8_t i = 0; i < sampleCount; ++i) {
    const long value = samples[i];
    sum += value;
    if (value < min_value) {
      min_value = value;
    }
    if (value > max_value) {
      max_value = value;
    }
  }

  if (sampleCount <= 2) {
    return static_cast<double>(sum) / static_cast<double>(sampleCount);
  }

  const long trimmed_sum = sum - min_value - max_value;
  return static_cast<double>(trimmed_sum) / static_cast<double>(sampleCount - 2);
}

double medianOfThree(double a, double b, double c) {
  if (a > b) {
    const double t = a;
    a = b;
    b = t;
  }
  if (b > c) {
    const double t = b;
    b = c;
    c = t;
  }
  if (a > b) {
    const double t = a;
    a = b;
    b = t;
  }
  return b;
}

double maxMinusMin(const double* values, uint8_t count) {
  if (count == 0) {
    return 0.0;
  }

  double min_value = values[0];
  double max_value = values[0];
  for (uint8_t i = 1; i < count; ++i) {
    if (values[i] < min_value) {
      min_value = values[i];
    }
    if (values[i] > max_value) {
      max_value = values[i];
    }
  }
  return max_value - min_value;
}

void warmupChannel(uint8_t channel) {
  for (uint8_t i = 0; i < kSensorActivationReads; ++i) {
    if (!waitForReady(channel)) {
      emitWarnChannel(F("warmup_timeout"), channel);
      return;
    }
    long raw = 0;
    if (!g_scales[channel].read_raw_if_ready(&raw)) {
      emitWarnChannel(F("warmup_timeout"), channel);
      return;
    }
    delay(10);
  }
}

bool calibrateChannel(uint8_t channel) {
  emitStatusChannelValue(F("calibration_channel_start"), channel, 0);

  for (uint8_t i = 0; i < kCalibrationWarmupReads; ++i) {
    if (!waitForReady(channel)) {
      emitWarnChannel(F("calibration_warmup_timeout"), channel);
      break;
    }
    long raw = 0;
    if (!g_scales[channel].read_raw_if_ready(&raw)) {
      emitWarnChannel(F("calibration_warmup_timeout"), channel);
      break;
    }
    delay(10);
  }

  double batch_means[kCalibrationBatchCount] = {0.0, 0.0, 0.0};
  long samples[kCalibrationBatchSampleCount];

  for (uint8_t batch = 0; batch < kCalibrationBatchCount; ++batch) {
    if (!collectReadings(channel, kCalibrationBatchSampleCount, samples, F("calibration_timeout"))) {
      emitWarnChannel(F("calibration_skipped"), channel);
      return false;
    }

    batch_means[batch] = trimmedMean(samples, kCalibrationBatchSampleCount);
    if (batch + 1 < kCalibrationBatchCount) {
      delay(kCalibrationBatchSettleDelayMs);
    }
  }

  const double batch_spread = maxMinusMin(batch_means, kCalibrationBatchCount);
  if (batch_spread > kCalibrationBatchSpreadThreshold) {
    emitWarnChannelValue(F("calibration_spread"), channel, batch_spread);
  }

  g_offsets[channel] = medianOfThree(batch_means[0], batch_means[1], batch_means[2]);
  g_previous_values[channel] = 0.0;

  emitStatusChannelValue(F("calibration_channel_end"), channel, lround(g_offsets[channel]));
  return true;
}

void initializeScales() {
  emitStatus(F("sensor_init_start"));

  for (uint8_t channel = 0; channel < kChannelCount; ++channel) {
    g_scales[channel].begin(kLoadcellDoutPins[channel], kLoadcellSckPins[channel]);
    warmupChannel(channel);
  }

  emitStatus(F("sensor_init_end"));
}

void calibrateAllChannels() {
  emitStatus(F("calibration_start"));

  for (uint8_t channel = 0; channel < kChannelCount; ++channel) {
    (void)calibrateChannel(channel);
  }

  emitStatus(F("calibration_end"));
}

void handleProvisionCommand(const char* device_id_text) {
  if (g_device_identity_valid) {
    emitWarn(F("already_provisioned"));
    return;
  }

  uint8_t requested_id[kIdentitySize];
  if (!parseDeviceId(device_id_text, requested_id)) {
    emitWarn(F("invalid_device_id"));
    return;
  }

  writeDeviceIdentity(requested_id);
  g_device_identity_valid = loadDeviceIdentity();

  if (!g_device_identity_valid || memcmp(g_device_id, requested_id, kIdentitySize) != 0) {
    emitWarn(F("provision_verify_failed"));
    g_device_identity_valid = false;
    return;
  }

  reloadSettings();
  invalidateTare();
  emitStatus(F("provision_ok"));
}

void handleStream3(const Command& command) {
  if (command.enabled) {
    const SessionStart started = g_v3_stream.start(command.session);
    if (started == SessionStart::Invalid) {
      emitError3(command.request, F("invalid_session"));
      return;
    }
    if (started == SessionStart::AlreadyActive) {
      emitOk3(command.request, F("stream"), g_v3_stream.challenge());
      return;
    }
    if (started == SessionStart::NewSession) {
      cancelTare(F("tare_cancelled"));
      invalidateTare();
    }
    g_v3_fresh_this_frame = 0;
    g_v3_sample_rejected = 0;
    g_v3_next_frame_us = micros() + kV3SamplePeriodMicros;
    g_v2_next_frame_us = micros() + kSamplePeriodMicros;
    emitOk3(command.request, F("stream"), g_v3_stream.challenge());
    return;
  }

  if (!g_v3_stream.stop(command.session)) {
    emitError3(command.request, F("stale_session"));
    return;
  }
  g_v3_next_frame_us = micros() + kV3SamplePeriodMicros;
  g_v2_next_frame_us = micros() + kSamplePeriodMicros;
  emitOk3(command.request, F("stream"), g_v3_stream.challenge());
}

void handleSettings3(const Command& command) {
  if (!g_device_identity_valid || !requestIdentityMatches(command)) {
    emitError3(command.request, F("device_mismatch"));
    return;
  }
  if (g_tare_active) {
    emitError3(command.request, F("busy"));
    return;
  }
  if (!settings_available()) {
    emitError3(command.request, F("settings_corrupt"));
    return;
  }
  if (command.expected_generation != current_generation()) {
    emitError3(command.request, F("stale_generation"));
    return;
  }

  Settings requested = {};
  requested.generation = command.expected_generation == 0 ? 1 : command.expected_generation;
  requested.assembly = command.assembly;
  memcpy(requested.identity, command.identity, sizeof(requested.identity));
  requested.known = command.known;
  requested.negative = command.negative;

  EepromStore store;
  const selfrionette::settings::SaveStatus saved = selfrionette::settings::save(
      store, g_device_id, requested, command.expected_generation);
  if (saved == selfrionette::settings::SaveStatus::Invalid) {
    emitError3(command.request, F("invalid_settings"));
    return;
  }
  if (saved == selfrionette::settings::SaveStatus::Stale) {
    emitError3(command.request, F("stale_generation"));
    return;
  }
  if (saved == selfrionette::settings::SaveStatus::Exhausted) {
    emitError3(command.request, F("generation_exhausted"));
    return;
  }
  if (saved == selfrionette::settings::SaveStatus::Unavailable ||
      saved == selfrionette::settings::SaveStatus::WriteFailed) {
    reloadSettings();
    emitError3(command.request, F("settings_write_failed"));
    return;
  }

  reloadSettings();
  if ((g_settings.state != selfrionette::settings::State::Usable &&
       g_settings.state != selfrionette::settings::State::Partial) ||
      !selfrionette::settings::same_payload(g_settings.value, requested)) {
    emitError3(command.request, F("settings_verify_failed"));
    return;
  }
  if (saved == selfrionette::settings::SaveStatus::Saved) invalidateTare();
  emitOk3(command.request, F("settings"), g_settings.value.generation);
}

void executeV3Command(const Command& command) {
  switch (command.kind) {
    case CommandKind::Info:
      emitDeviceInfo3(command.request);
      break;
    case CommandKind::Stream:
      handleStream3(command);
      break;
    case CommandKind::Settings:
      handleSettings3(command);
      break;
    case CommandKind::Tare:
      beginTare(command);
      break;
    case CommandKind::None:
      break;
  }
}

void executeLegacyCommand(char* command) {
  if (strcmp(command, "info") == 0 || strcmp(command, "?2") == 0) {
    emitDeviceInfo();
    return;
  }

  if (strcmp(command, "tare") == 0 || strcmp(command, "c") == 0) {
    cancelTare(F("tare_cancelled"));
    invalidateTare();
    emitStatus(F("tare_command_received"));
    calibrateAllChannels();
    return;
  }

  constexpr char kProvisionPrefix[] = "provision,";
  constexpr size_t kProvisionPrefixLength = sizeof(kProvisionPrefix) - 1;
  if (strncmp(command, kProvisionPrefix, kProvisionPrefixLength) == 0) {
    handleProvisionCommand(command + kProvisionPrefixLength);
    return;
  }

  if (command[0] != '\0') {
    emitWarn(F("unknown_command"));
  }
}

void finishCommand() {
  if (g_command_overflow) {
    emitWarn(F("command_too_long"));
  } else if (g_command_length != 0) {
    g_command_buffer[g_command_length] = '\0';
    if (selfrionette::protocol3::is_v3_payload(g_command_buffer)) {
      Command command = {};
      const ParseStatus parsed = selfrionette::protocol3::parse_command(
          g_command_buffer, command);
      if (parsed == ParseStatus::Valid) executeV3Command(command);
      else if (parsed == ParseStatus::Malformed && command.has_request) {
        emitError3(command.request, F("malformed_command"));
      }
    } else if (g_command_length > 63) {
      emitWarn(F("command_too_long"));
    } else {
      executeLegacyCommand(g_command_buffer);
    }
  }

  g_command_length = 0;
  g_command_overflow = false;
}

void handleSerialCommands() {
  if (g_command_ready) {
    if (!g_output.canQueue(kCommandReplyReserveBytes)) return;
    g_command_ready = false;
    finishCommand();
    return;
  }

  uint8_t consumed = 0;
  while (Serial.available() > 0 && consumed < 32) {
    const char value = static_cast<char>(Serial.read());
    ++consumed;

    if (value == '\r') {
      continue;
    }

    if (value == '\n') {
      g_command_ready = true;
      break;
    }

    if (g_command_overflow) {
      continue;
    }

    if (static_cast<uint16_t>(g_command_length) + 1U >= kCommandBufferSize) {
      g_command_overflow = true;
      continue;
    }

    g_command_buffer[g_command_length++] = value;
  }
}

}  // namespace

void setup() {
  static_assert(sizeof(long) == 4, "firmware assumes 32-bit AVR long");
  static_assert(sizeof(double) == 4, "firmware memory budget assumes AVR 32-bit double");
  static_assert(kIdentityBaseAddress + kIdentityRecordSize <= E2END + 1, "identity record exceeds EEPROM");

  Serial.begin(kSerialBaudRate);
  emitStatus(F("setup_start"));

  g_device_identity_valid = loadDeviceIdentity();
  reloadSettings();
  initializeScales();
  calibrateAllChannels();

  const uint32_t now_us = micros();
  g_v2_next_frame_us = now_us + kSamplePeriodMicros;
  g_v3_next_frame_us = now_us + kV3SamplePeriodMicros;

  emitStatus(F("setup_end"));
}

void loop() {
  g_output.drain();
  handleSerialCommands();
  updateSensors();

  const uint32_t now_us = micros();
  if (g_v3_stream.active()) {
    if (selfrionette::firmware::deadline_reached(now_us, g_v3_next_frame_us)) {
      emitSample3(millis());
      g_v3_next_frame_us = selfrionette::firmware::next_deadline(now_us);
    }
  } else if (static_cast<int32_t>(now_us - g_v2_next_frame_us) >= 0) {
    printVectorLine(millis(), g_current_values);
    g_v2_next_frame_us = now_us + kSamplePeriodMicros;
  }

  g_output.drain();
  delayMicroseconds(250);
}
