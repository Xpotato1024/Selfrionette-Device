// Software-only execution of the current firmware path; every hardware API is
// a deterministic fake. This measures no ADC, USB timing, or AVR CPU throughput.
#include "../shared/buffered_line_queue.h"
#include "../shared/device_settings.h"
#include "../shared/firmware_protocol_v3.h"
#include "../shared/stream_cadence.h"
#include <algorithm>
#include <cmath>
#include <cstdio>
#include <cstdlib>
#include <cstring>
#include <string>
#include <vector>

#define CHECK(x) do { if (!(x)) { fprintf(stderr, "FAIL line %d: %s\n", __LINE__, #x); exit(1); } } while (0)
#define SELFRIONETTE_SOURCE_SHA "0123456789abcdef0123456789abcdef01234567"
#define SELFRIONETTE_FIRMWARE_VERSION "1.1.0-preview.1"

class __FlashStringHelper;
#define F(text) reinterpret_cast<const __FlashStringHelper*>(text)

class Print {
 public:
  virtual ~Print() {}
  virtual size_t write(uint8_t value) = 0;
  size_t print(const __FlashStringHelper* text) {
    const char* value = reinterpret_cast<const char*>(text);
    size_t written = 0;
    while (*value) written += write(static_cast<uint8_t>(*value++));
    return written;
  }
  size_t print(char value) { return write(static_cast<uint8_t>(value)); }
  size_t print(uint8_t value) { return print(static_cast<unsigned long>(value)); }
  size_t print(int value) { return print(static_cast<long>(value)); }
  size_t print(unsigned int value) { return print(static_cast<unsigned long>(value)); }
  size_t print(long value) { return printNumber(std::to_string(value)); }
  size_t print(unsigned long value) { return printNumber(std::to_string(value)); }
  size_t print(double value) {
    char text[48];
    snprintf(text, sizeof(text), "%.2f", value);
    return printNumber(text);
  }
 private:
  size_t printNumber(const std::string& value) {
    size_t written = 0;
    for (char byte : value) written += write(static_cast<uint8_t>(byte));
    return written;
  }
};

struct SerialFake {
  std::string bytes;
  std::string input;
  size_t input_at = 0;
  int writable = 64;
  size_t max_write = 64;
  bool zero_progress = false;
  unsigned writes = 0;
  int available() const { return static_cast<int>(input.size() - input_at); }
  char read() { return input[input_at++]; }
  int availableForWrite() const { return writable; }
  void feed(const std::string& command) { input = command; input_at = 0; }
  size_t write(const uint8_t* data, size_t count) {
    ++writes;
    if (zero_progress) return 0;
    const size_t accepted = std::min(count, std::min(max_write, static_cast<size_t>(writable)));
    bytes.append(reinterpret_cast<const char*>(data), accepted);
    return accepted;
  }
} Serial;

uint64_t clock_us = 0;
uint32_t micros() { return static_cast<uint32_t>(clock_us); }
uint32_t millis() { return static_cast<uint32_t>(clock_us / 1000U); }
void delay(unsigned long value) { clock_us += value * 1000U; }
void delayMicroseconds(unsigned long value) { clock_us += value; }

struct EepromFake {
  uint8_t bytes[1024] = {};
  unsigned writes = 0;
  uint8_t read(uint16_t address) const { CHECK(address < sizeof(bytes)); return bytes[address]; }
  void update(uint16_t address, uint8_t value) { CHECK(address < sizeof(bytes)); bytes[address] = value; ++writes; }
} EEPROM;

struct SensorFake {
  bool ready;
  long raw;
  SensorFake(bool is_ready = false, long value = 0) : ready(is_ready), raw(value) {}
} sensors[7];
unsigned next_sensor = 0;
class HX711 {
 public:
  HX711() : channel_(next_sensor++) { CHECK(channel_ < 7); }
  void begin(uint8_t, uint8_t) {}
  bool wait_ready_timeout(unsigned long, unsigned long) { return sensors[channel_].ready; }
  bool read_raw_if_ready(long* value) {
    if (!sensors[channel_].ready) return false;
    *value = sensors[channel_].raw;
    sensors[channel_].ready = false;
    return true;
  }
 private:
  unsigned channel_;
};

// Generated from main.cpp by tools/test-firmware-quality-stream.mjs.
#include "quality-stream-under-test.inc"

using selfrionette::firmware::kQualityStreamPeriodMicros;
using selfrionette::firmware::kQualityStreamRateHz;
using namespace selfrionette::protocol3;

struct Frame {
  std::vector<std::string> fields;
  uint32_t number(size_t field) const { return static_cast<uint32_t>(std::stoul(fields.at(field))); }
  int32_t raw(size_t channel) const { return static_cast<int32_t>(std::stol(fields.at(7 + channel))); }
  uint8_t flags(size_t channel) const { return static_cast<uint8_t>(number(21 + channel)); }
};

std::vector<Frame> frames() {
  std::vector<Frame> result;
  size_t start = 0;
  while (start < Serial.bytes.size()) {
    const size_t end = Serial.bytes.find('\n', start);
    CHECK(end != std::string::npos);
    const std::string line = Serial.bytes.substr(start, end - start);
    start = end + 1;
    if (line.compare(0, 8, "sample3,") != 0) continue;
    Frame frame;
    size_t field_start = 0;
    while (field_start <= line.size()) {
      const size_t comma = line.find(',', field_start);
      frame.fields.push_back(line.substr(field_start, comma - field_start));
      if (comma == std::string::npos) break;
      field_start = comma + 1;
    }
    CHECK(frame.fields.size() == 28);
    result.push_back(frame);
  }
  return result;
}

void reset_firmware(uint64_t now_us = 0) {
  clock_us = now_us;
  Serial = SerialFake();
  EEPROM = EepromFake();
  g_output = BufferedSerialOutput();
  g_v3_stream = StreamSession();
  g_v3_last_attempt_us = micros();
  g_v2_next_frame_us = micros() + kSamplePeriodMicros;
  g_settings = Loaded();
  g_settings.state = selfrionette::settings::State::Usable;
  g_settings.value.generation = 1;
  g_settings.value.assembly = 1;
  g_settings.value.known = 127;
  memset(g_device_id, 0, sizeof(g_device_id));
  g_device_id[0] = 1;
  memcpy(g_settings.value.identity, g_device_id, sizeof(g_device_id));
  g_device_identity_valid = true;
  memset(g_v3_raw, 0, sizeof(g_v3_raw));
  memset(g_v3_last_read_ms, 0, sizeof(g_v3_last_read_ms));
  memset(g_offsets, 0, sizeof(g_offsets));
  memset(g_current_values, 0, sizeof(g_current_values));
  memset(g_previous_values, 0, sizeof(g_previous_values));
  memset(g_command_buffer, 0, sizeof(g_command_buffer));
  g_command_length = 0;
  g_command_overflow = g_command_ready = false;
  g_v3_has_raw = g_v3_fresh_this_frame = g_v3_sample_rejected = g_v3_rejected_next = 0;
  g_legacy_timeout_warned = 127;
  g_tare_active = g_tare_valid = false;
  g_tare_epoch = g_tare_request = g_tare_started_ms = 0;
  memset(g_tare_zero, 0, sizeof(g_tare_zero));
  g_tare_window.reset();
  for (SensorFake& sensor : sensors) sensor = SensorFake();
}

void stream_command(uint32_t challenge, bool enabled) {
  Command command = {};
  command.request = 17;
  command.session = challenge;
  command.enabled = enabled;
  handleStream3(command);
}

void start_stream(uint32_t challenge = 7) {
  stream_command(challenge, true);
  CHECK(g_v3_stream.active());
  Serial.bytes.clear();
}

void run_at(uint64_t now_us, uint8_t ready = 0, long raw = 100) {
  clock_us = now_us;
  for (uint8_t channel = 0; channel < 7; ++channel) {
    if ((ready & (1U << channel)) != 0) sensors[channel] = {true, raw + channel};
  }
  loop();
}

void cadence_and_conversion_contract() {
  const uint32_t period = kQualityStreamPeriodMicros;
  reset_firmware();
  start_stream();
  run_at(period - 1, 127);
  CHECK(frames().empty());
  run_at(period);
  CHECK(frames().size() == 1 && frames()[0].flags(0) == kFresh);
  run_at(period * 2U - 1);
  CHECK(frames().size() == 1);
  run_at(period * 2U);
  CHECK(frames().size() == 2);
  CHECK((frames()[1].flags(0) & kFresh) == 0);
  CHECK(frames()[1].raw(0) == frames()[0].raw(0));
  CHECK(frames()[1].fields[14] == "?");

  for (uint32_t conversion_rate : {10U, 80U}) {
    reset_firmware();
    start_stream();
    g_tare_valid = true;  // Synthetic successful session tare; physical tare is not implied.
    g_tare_epoch = 1;
    const uint32_t conversion_period = 1000000U / conversion_rate;
    for (uint32_t now = 250; now <= 1000000; now += 250) {
      // New ADC conversions may have identical count values; sending the same
      // stored reading again without a conversion must never become fresh.
      run_at(now, now % conversion_period == 0 ? 127 : 0, 100);
    }
    const std::vector<Frame> output = frames();
    CHECK(output.size() == kQualityStreamRateHz);
    const uint32_t expected_fresh = std::min(conversion_rate, kQualityStreamRateHz);
    for (uint8_t channel = 0; channel < 7; ++channel) {
      unsigned fresh = 0;
      for (const Frame& frame : output) fresh += frame.flags(channel) == kFresh;
      CHECK(fresh == expected_fresh);
    }
    unsigned all_usable = 0;
    for (const Frame& frame : output) {
      bool usable = frame.number(6) == 1;
      for (uint8_t channel = 0; channel < 7; ++channel) {
        usable = usable && frame.flags(channel) == kFresh && frame.fields[14 + channel] != "?";
      }
      all_usable += usable;
    }
    CHECK(all_usable == expected_fresh);
    CHECK(EEPROM.writes == 0);
    printf("synthetic: frame_rate=%lu ADC_rate=%lu each_channel_fresh=%lu all_usable=%u\n",
           static_cast<unsigned long>(kQualityStreamRateHz),
           static_cast<unsigned long>(conversion_rate),
           static_cast<unsigned long>(expected_fresh), all_usable);
  }
}

void long_stall_and_rollover_contract() {
  const uint64_t start = static_cast<uint64_t>(UINT32_MAX) - 10000U;
  const uint32_t period = kQualityStreamPeriodMicros;
  reset_firmware(start);
  start_stream();
  run_at(start + period - 1, 127);
  CHECK(frames().empty());
  run_at(start + period, 127);
  CHECK(frames().size() == 1);
  const uint64_t after_stall = start + period + UINT32_C(0x80000000);
  run_at(after_stall, 127, 200);
  CHECK(frames().size() == 2);
  run_at(after_stall + 1, 127, 201);
  run_at(after_stall + period - 1, 127, 202);
  CHECK(frames().size() == 2);
  run_at(after_stall + period, 127, 203);
  CHECK(frames().size() == 3 && frames().back().raw(0) == 203);
}

void bounded_backpressure_contract() {
  const uint32_t period = kQualityStreamPeriodMicros;
  for (bool zero_progress : {false, true}) {
    reset_firmware();
    start_stream();
    Serial.writable = zero_progress ? 64 : 0;
    Serial.zero_progress = zero_progress;
    for (uint32_t attempt = 1; attempt <= 10; ++attempt) run_at(attempt * period, 127, attempt * 100);
    CHECK(Serial.bytes.empty());
    CHECK(g_output.busy() && g_v3_stream.sequence() == 1);
    CHECK(g_v3_raw[0] == 1000 && (g_v3_fresh_this_frame & 1) != 0);
    Serial.writable = 64;
    Serial.zero_progress = false;
    Serial.max_write = 1;  // Force every queued line through short writes.
    run_at(10U * period + 1, 127, 2000);
    CHECK(frames().size() == 1 && frames()[0].raw(0) == 100);
    CHECK(frames()[0].number(3) == period / 1000U);  // Never retimestamp a held queue.
    run_at(11U * period, 127, 3000);
    const std::vector<Frame> output = frames();
    CHECK(output.size() == 2 && output[1].raw(0) == 3000);
    CHECK(output[0].number(2) == 1 && output[1].number(2) == 2);
    CHECK(output[1].flags(0) == kFresh && !g_output.busy());
    CHECK(EEPROM.writes == 0);
  }
}

void command_reserve_and_pause_contract() {
  const uint32_t period = kQualityStreamPeriodMicros;
  reset_firmware();
  start_stream();
  Serial.writable = 0;
  run_at(period, 127);
  Serial.feed("stream3,19,7,0\n");
  run_at(period + 250);
  CHECK(g_command_ready && g_v3_stream.active());
  run_at(2U * period, 127, 200);
  CHECK(g_command_ready && g_v3_stream.active() && g_v3_stream.sequence() == 1);
  Serial.writable = 64;
  run_at(2U * period + 250);
  CHECK(!g_command_ready && !g_v3_stream.active());
  CHECK(Serial.bytes.find("ok3,19,stream,7\n") != std::string::npos);
  CHECK(frames().size() == 1);

  g_tare_valid = true;
  g_tare_epoch = 3;
  run_at(3U * period, 127, 300);  // ADC conversions during pause are not fresh after resume.
  const uint64_t resume_us = clock_us;
  stream_command(7, true);
  CHECK(g_tare_valid && g_tare_epoch == 3 && g_v3_stream.sequence() == 1);
  CHECK(g_v3_fresh_this_frame == 0 && g_v3_sample_rejected == 0);
  run_at(resume_us + period);
  CHECK(frames().size() == 2 && (frames().back().flags(0) & kFresh) == 0);
  stream_command(8, true);
  CHECK(!g_tare_valid && g_v3_stream.sequence() == 0);
  const uint64_t new_session_us = clock_us;
  run_at(new_session_us + period, 127, 400);
  CHECK(frames().back().number(1) == 8 && frames().back().number(2) == 1);
  CHECK(EEPROM.writes == 0);
}

void tare_and_mixed_quality_contract() {
  const uint32_t period = kQualityStreamPeriodMicros;
  reset_firmware();
  start_stream();
  Command command = {};
  command.request = 42;
  command.expected_generation = 1;
  memcpy(command.identity, g_device_id, sizeof(command.identity));
  beginTare(command);
  CHECK(g_tare_active && !g_tare_valid);
  for (uint32_t sample = 1; sample <= 8; ++sample) run_at(sample * period, 127, 99 + sample);
  CHECK(!g_tare_active && g_tare_valid && g_tare_epoch == 1);
  CHECK(Serial.bytes.find("ok3,42,tare,1\n") != std::string::npos);
  CHECK(frames().back().fields[14] == "4");
  beginTare(command);
  stream_command(8, true);
  CHECK(!g_tare_active && !g_tare_valid);
  CHECK(Serial.bytes.find("error3,42,tare_cancelled\n") != std::string::npos);

  reset_firmware(1000000U);
  start_stream();
  g_tare_valid = true;
  g_tare_epoch = 1;
  const uint32_t sample_ms = (1000000U + period) / 1000U;
  g_v3_has_raw = 6;
  g_v3_raw[1] = 20;
  g_v3_raw[2] = 30;
  g_v3_last_read_ms[1] = sample_ms - 1;
  g_v3_last_read_ms[2] = sample_ms - kFreshTimeoutMs;
  g_v3_rejected_next = 1U << 5;
  sensors[0] = {true, 10};
  sensors[3] = {true, 8388607L};
  sensors[4] = {true, -8388608L};
  sensors[5] = {true, 60};
  run_at(1000000U + period);
  const std::vector<Frame> output = frames();
  CHECK(output.size() == 1);
  const uint8_t expected[] = {kFresh, kHeld, kHeld | kTimeout,
                             kFresh | kSaturated, kFresh | kSaturated,
                             kFresh | kRejected, kTimeout};
  for (uint8_t channel = 0; channel < 7; ++channel) {
    CHECK(output[0].flags(channel) == expected[channel]);
    CHECK(output[0].fields[14 + channel] == (channel == 0 ? "10" : "?"));
  }
  CHECK(output[0].fields[13] == "?" && EEPROM.writes == 0);
}

void asynchronous_reply_reserve_contract() {
  reset_firmware();
  start_stream();
  Command command = {};
  command.request = UINT32_MAX;
  command.expected_generation = 1;
  memcpy(command.identity, g_device_id, sizeof(command.identity));
  beginTare(command);
  Serial.writable = 0;
  run_at(kQualityStreamPeriodMicros, 1, 100);
  CHECK(g_output.busy() && g_v3_stream.sequence() == 1);
  run_at(kTareTimeoutMs * 1000U);
  CHECK(!g_tare_active && !g_tare_valid && g_v3_stream.sequence() == 1);
  Serial.writable = 64;
  g_output.drain();
  CHECK(Serial.bytes.find("error3,4294967295,tare_timeout\n") != std::string::npos);
  CHECK(frames().size() == 1 && !g_output.busy());
  CHECK(EEPROM.writes == 0);

  reset_firmware();
  g_settings.value.generation = g_settings.value.assembly = UINT32_MAX;
  g_settings.value.negative = 127;
  g_tare_epoch = UINT32_MAX;
  g_tare_valid = true;
  emitDeviceInfo3(UINT32_MAX);
  CHECK(Serial.bytes.size() == 179U + strlen(SELFRIONETTE_FIRMWARE_VERSION));
  CHECK(Serial.bytes.size() <= kInfo3MaxLineBytes);
  CHECK(!g_output.busy());
}

int main() {
  cadence_and_conversion_contract();
  long_stall_and_rollover_contract();
  bounded_backpressure_contract();
  command_reserve_and_pause_contract();
  tare_and_mixed_quality_contract();
  asynchronous_reply_reserve_contract();
  puts("PASS: actual Managed loop, cadence, ADC quality, bounded USB queue, commands, session, and tare (software-only)");
}
