#include <Arduino.h>
#include <EEPROM.h>
#include <math.h>
#include <string.h>

#include "HX711.h"

namespace {
#define SELFRIONETTE_FIRMWARE_VERSION "0.1.0"

constexpr uint8_t kProtocolMajorVersion = 2;
constexpr uint8_t kChannelCount = 7;
constexpr unsigned long kSerialBaudRate = 115200UL;
constexpr unsigned long kSampleRateHz = 80UL;
constexpr unsigned long kSamplePeriodMicros = 1000000UL / kSampleRateHz;
constexpr unsigned long kReadyTimeoutMs = 10UL;
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

constexpr uint8_t kCommandBufferSize = 64;
char g_command_buffer[kCommandBufferSize];
uint8_t g_command_length = 0;
bool g_command_overflow = false;

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

void emitStatus(const __FlashStringHelper* token) {
  Serial.print(F("status,"));
  Serial.println(token);
}

void emitStatusChannelValue(const __FlashStringHelper* token, uint8_t channel, long value) {
  Serial.print(F("status,"));
  Serial.print(token);
  Serial.print(',');
  Serial.print(channel);
  Serial.print(',');
  Serial.println(value);
}

void emitWarn(const __FlashStringHelper* token) {
  Serial.print(F("warn,"));
  Serial.println(token);
}

void emitWarnChannel(const __FlashStringHelper* token, uint8_t channel) {
  Serial.print(F("warn,"));
  Serial.print(token);
  Serial.print(',');
  Serial.println(channel);
}

void emitWarnChannelValue(const __FlashStringHelper* token, uint8_t channel, double value) {
  Serial.print(F("warn,"));
  Serial.print(token);
  Serial.print(',');
  Serial.print(channel);
  Serial.print(',');
  Serial.println(value);
}

void printVectorLine(unsigned long timestamp_ms, const double* values) {
  Serial.print(F("vector,"));
  Serial.print(timestamp_ms);
  for (uint8_t i = 0; i < kChannelCount; ++i) {
    Serial.print(',');
    Serial.print(values[i]);
  }
  Serial.println();
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
  Serial.print(F("srn-"));
  for (uint8_t i = 0; i < kIdentitySize; ++i) {
    Serial.print(hexDigit(static_cast<uint8_t>(id[i] >> 4)));
    Serial.print(hexDigit(static_cast<uint8_t>(id[i] & 0x0F)));
  }
}

void emitDeviceInfo() {
  Serial.print(F("device,"));
  Serial.print(kProtocolMajorVersion);
  Serial.print(',');
  Serial.print(F(SELFRIONETTE_FIRMWARE_VERSION));
  Serial.print(',');
  if (g_device_identity_valid) {
    printDeviceId(g_device_id);
  } else {
    Serial.print(F("unprovisioned"));
  }
  Serial.print(',');
  Serial.println(kChannelCount);
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

bool waitForReady(uint8_t channel) {
  return g_scales[channel].wait_ready_timeout(kReadyTimeoutMs, kReadyPollDelayMs);
}

long readSignedReading(uint8_t channel) {
  return -g_scales[channel].read();
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

    samples[i] = readSignedReading(channel);
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
    (void)g_scales[channel].read();
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
    (void)g_scales[channel].read();
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

  emitStatus(F("provision_ok"));
}

void executeCommand(char* command) {
  if (strcmp(command, "info") == 0) {
    emitDeviceInfo();
    return;
  }

  if (strcmp(command, "tare") == 0 || strcmp(command, "c") == 0) {
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
  } else {
    g_command_buffer[g_command_length] = '\0';
    executeCommand(g_command_buffer);
  }

  g_command_length = 0;
  g_command_overflow = false;
}

void handleSerialCommands() {
  while (Serial.available() > 0) {
    const char value = static_cast<char>(Serial.read());

    if (value == '\r') {
      continue;
    }

    if (value == '\n') {
      finishCommand();
      continue;
    }

    if (g_command_overflow) {
      continue;
    }

    if (g_command_length + 1 >= kCommandBufferSize) {
      g_command_overflow = true;
      continue;
    }

    g_command_buffer[g_command_length++] = value;
  }
}

bool readChannelValue(uint8_t channel, double* value) {
  if (!waitForReady(channel)) {
    emitWarnChannel(F("ready_timeout"), channel);
    *value = g_previous_values[channel];
    return false;
  }

  const double reading = readSignedReading(channel);
  const double adjusted = reading - g_offsets[channel];

  if (fabs(adjusted - g_previous_values[channel]) > kMaxChangeThreshold) {
    *value = g_previous_values[channel];
    emitWarnChannelValue(F("spike"), channel, *value);
  } else {
    *value = adjusted;
  }

  g_previous_values[channel] = *value;
  return true;
}

void updateAllValues() {
  for (uint8_t channel = 0; channel < kChannelCount; ++channel) {
    (void)readChannelValue(channel, &g_current_values[channel]);
  }
}
}  // namespace

void setup() {
  static_assert(kIdentityBaseAddress + kIdentityRecordSize <= E2END + 1, "identity record exceeds EEPROM");

  Serial.begin(kSerialBaudRate);
  emitStatus(F("setup_start"));

  g_device_identity_valid = loadDeviceIdentity();
  initializeScales();
  calibrateAllChannels();

  emitStatus(F("setup_end"));
}

void loop() {
  const unsigned long cycle_start_us = micros();

  handleSerialCommands();
  updateAllValues();
  printVectorLine(millis(), g_current_values);

  while (micros() - cycle_start_us < kSamplePeriodMicros) {
  }
}
