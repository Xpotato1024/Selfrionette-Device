#ifndef SELFRIONETTE_DEVICE_SETTINGS_H
#define SELFRIONETTE_DEVICE_SETTINGS_H

// Hardware-independent schema/journal. The v2 stream does not use this yet.
#include <stdint.h>
#include <string.h>

namespace selfrionette { namespace settings {
constexpr uint16_t kBase = 32;
constexpr uint8_t kSize = 64;
constexpr uint8_t kChannels = 7;
constexpr uint8_t kAllKnown = 0x7f;
constexpr uint8_t kCommitted = 0xa5;
static_assert(kBase + 2 * kSize <= 1024, "settings exceed ATmega32U4 EEPROM");

struct Settings {
  uint32_t generation;
  uint32_t assembly;
  uint8_t identity[16];
  uint8_t known;
  uint8_t negative;
};
enum class State { Empty, Partial, Usable, Corrupt, Ambiguous };
struct Loaded {
  State state;
  uint8_t slot;
  Settings value;
};
enum class SaveStatus { Saved, Unchanged, Invalid, Stale, Unavailable, Exhausted, WriteFailed };

inline bool valid(const Settings& value) {
  uint8_t combined = 0;
  for (uint8_t i = 0; i < 16; ++i) combined |= value.identity[i];
  return combined != 0 && value.generation != 0 && value.assembly != 0 &&
         (value.known & 0x80) == 0 && (value.negative & ~value.known) == 0;
}
inline bool same_payload(const Settings& a, const Settings& b) {
  return a.assembly == b.assembly && a.known == b.known && a.negative == b.negative &&
         memcmp(a.identity, b.identity, 16) == 0;
}
inline void put32(uint8_t* output, uint32_t value) {
  for (uint8_t i = 0; i < 4; ++i) output[i] = static_cast<uint8_t>(value >> (8 * i));
}
inline uint32_t get32(const uint8_t* input) {
  uint32_t value = 0;
  for (uint8_t i = 0; i < 4; ++i) value |= static_cast<uint32_t>(input[i]) << (8 * i);
  return value;
}
inline uint16_t crc16(const uint8_t* bytes, uint8_t length) {
  uint16_t crc = 0xffff;
  for (uint8_t i = 0; i < length; ++i) {
    crc ^= static_cast<uint16_t>(bytes[i]) << 8;
    for (uint8_t bit = 0; bit < 8; ++bit)
      crc = static_cast<uint16_t>((crc & 0x8000) ? (crc << 1) ^ 0x1021 : crc << 1);
  }
  return crc;
}
inline bool encode(const Settings& value, uint8_t (&output)[kSize]) {
  if (!valid(value)) return false;
  memset(output, 0, kSize);
  memcpy(output, "SRNC", 4);
  output[4] = 1; output[5] = kSize; output[6] = kChannels; output[7] = 1;
  put32(output + 8, value.generation); put32(output + 12, value.assembly);
  memcpy(output + 16, value.identity, 16);
  output[32] = value.known; output[33] = value.negative;
  const uint16_t crc = crc16(output, 61);
  output[61] = static_cast<uint8_t>(crc); output[62] = static_cast<uint8_t>(crc >> 8);
  output[63] = kCommitted;
  return true;
}
inline bool decode(const uint8_t (&input)[kSize], const uint8_t (&identity)[16], Settings& output) {
  if (input[63] != kCommitted || memcmp(input, "SRNC", 4) != 0 || input[4] != 1 ||
      input[5] != kSize || input[6] != kChannels || input[7] != 1 ||
      memcmp(input + 16, identity, 16) != 0) return false;
  for (uint8_t i = 34; i < 61; ++i) if (input[i] != 0) return false;
  const uint16_t stored = static_cast<uint16_t>(input[61]) |
                          (static_cast<uint16_t>(input[62]) << 8);
  if (stored != crc16(input, 61)) return false;
  Settings candidate = {};
  candidate.generation = get32(input + 8); candidate.assembly = get32(input + 12);
  memcpy(candidate.identity, input + 16, 16);
  candidate.known = input[32]; candidate.negative = input[33];
  if (!valid(candidate)) return false;
  output = candidate;
  return true;
}
// Store exposes read(address)->byte and update(address, byte)->bool.
// A failed update must not be reported as successful by an eventual hardware adapter.
template<class Store>
Loaded load(Store& store, const uint8_t (&identity)[16]) {
  Loaded result = {};
  result.state = State::Empty;
  bool found = false;
  for (uint8_t slot = 0; slot < 2; ++slot) {
    const uint16_t base = kBase + slot * kSize;
    const uint8_t marker = store.read(base + 63);
    if (marker != kCommitted) {
      if (marker == 0) continue; // explicitly invalidated incomplete write
      if (marker == 0xff) {
        bool erased = true;
        for (uint8_t i = 0; i < 63; ++i) erased = erased && store.read(base + i) == 0xff;
        if (erased) continue;
      }
      result.state = State::Corrupt; return result;
    }
    uint8_t bytes[kSize];
    for (uint8_t i = 0; i < kSize; ++i) bytes[i] = store.read(base + i);
    Settings candidate = {};
    if (!decode(bytes, identity, candidate)) { result.state = State::Corrupt; return result; }
    if (found && candidate.generation == result.value.generation) {
      result.state = State::Ambiguous; return result;
    }
    if (!found || candidate.generation > result.value.generation) {
      result.value = candidate; result.slot = slot;
    }
    found = true;
  }
  if (found) result.state = result.value.known == kAllKnown ? State::Usable : State::Partial;
  return result;
}
template<class Store>
SaveStatus save(Store& store, const uint8_t (&identity)[16], const Settings& requested,
                uint32_t expected_generation) {
  // requested.generation is validated but never trusted as the next generation.
  if (!valid(requested) || memcmp(identity, requested.identity, 16) != 0) return SaveStatus::Invalid;
  const Loaded previous = load(store, identity);
  if (previous.state == State::Corrupt || previous.state == State::Ambiguous) return SaveStatus::Unavailable;
  const bool exists = previous.state != State::Empty;
  const uint32_t generation = exists ? previous.value.generation : 0;
  if (generation != expected_generation) return SaveStatus::Stale;
  if (exists && same_payload(previous.value, requested)) return SaveStatus::Unchanged;
  if (generation == UINT32_MAX) return SaveStatus::Exhausted;
  Settings candidate = requested;
  candidate.generation = generation + 1;
  uint8_t bytes[kSize];
  if (!encode(candidate, bytes)) return SaveStatus::Invalid;
  const uint16_t base = kBase + (exists ? (1 - previous.slot) * kSize : 0);
  if (!store.update(base + 63, 0)) return SaveStatus::WriteFailed;
  for (uint8_t i = 0; i < 63; ++i)
    if (!store.update(base + i, bytes[i])) return SaveStatus::WriteFailed;
  for (uint8_t i = 0; i < 63; ++i)
    if (store.read(base + i) != bytes[i]) return SaveStatus::WriteFailed;
  if (!store.update(base + 63, kCommitted)) return SaveStatus::WriteFailed;
  const Loaded after = load(store, identity);
  if ((after.state != State::Usable && after.state != State::Partial) ||
      after.value.generation != candidate.generation || !same_payload(after.value, candidate))
    return SaveStatus::WriteFailed;
  return SaveStatus::Saved;
}
// No unit conversion, robot mapping, channel permutation, tare acquisition, or saturation.
inline bool orient_count(const Loaded& loaded, uint8_t channel, int32_t adc,
                          int32_t zero, int32_t& output) {
  const Settings& value = loaded.value;
  if (loaded.state != State::Usable || !valid(value) || value.known != kAllKnown || channel >= kChannels ||
      adc < -8388608L || adc > 8388607L || zero < -8388608L || zero > 8388607L) return false;
  const int32_t difference = adc - zero; // fits i32 even at opposite 24-bit extremes
  output = (value.negative & (1u << channel)) ? -difference : difference;
  return true;
}
}} // namespace selfrionette::settings
#endif
