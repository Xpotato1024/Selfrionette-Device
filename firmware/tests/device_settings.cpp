// Synthetic byte store only. Never opens serial or writes device memory.
#include "../shared/device_settings.h"
#include <stdio.h>
#include <stdlib.h>
#include <limits.h>
using namespace selfrionette::settings;
#define CHECK(x) do { if (!(x)) { fprintf(stderr, "FAIL line %d: %s\n", __LINE__, #x); exit(1); } } while (0)
struct Store {
  uint8_t bytes[1024];
  int remaining;
  unsigned calls;
  bool drop;
  bool fail_after_commit;
  Store() : remaining(-1), calls(0), drop(false), fail_after_commit(false) { memset(bytes, 0xff, sizeof(bytes)); }
  uint8_t read(uint16_t address) const { CHECK(address < sizeof(bytes)); return bytes[address]; }
  bool update(uint16_t address, uint8_t value) {
    CHECK(address >= kBase && address < kBase + 2 * kSize);
    if (remaining == 0) return false;
    if (remaining > 0) --remaining;
    ++calls;
    if (!drop) bytes[address] = value;
    return !(fail_after_commit && value == kCommitted && address % kSize == (kBase + 63) % kSize);
  }
};
Settings fixture() {
  Settings value = {};
  value.generation = 1; value.assembly = 0x01020304;
  for (uint8_t i = 0; i < 16; ++i) value.identity[i] = i + 1;
  value.known = kAllKnown; value.negative = 0x12;
  return value;
}
void place(Store& store, uint8_t slot, const Settings& value) {
  uint8_t bytes[kSize]; CHECK(encode(value, bytes));
  memcpy(store.bytes + kBase + slot * kSize, bytes, kSize);
}
void golden() {
  Settings value = fixture(); uint8_t bytes[kSize]; CHECK(encode(value, bytes));
  CHECK(memcmp(bytes, "SRNC", 4) == 0);
  CHECK(bytes[4] == 1 && bytes[5] == 64 && bytes[6] == 7 && bytes[7] == 1);
  CHECK(bytes[12] == 4 && bytes[13] == 3 && bytes[14] == 2 && bytes[15] == 1);
  // Independent Python binascii.crc_hqx(bytes[:61], 0xffff) golden.
  CHECK(bytes[61] == 0xcc && bytes[62] == 0x39 && bytes[63] == 0xa5);
  Settings decoded = {}; CHECK(decode(bytes, value.identity, decoded));
  CHECK(same_payload(value, decoded) && decoded.generation == 1);
  for (uint8_t i = 0; i < kSize; ++i) {
    uint8_t damaged[kSize]; memcpy(damaged, bytes, kSize); damaged[i] ^= 1;
    CHECK(!decode(damaged, value.identity, decoded));
  }
  uint8_t other[16]; memcpy(other, value.identity, 16); other[0] ^= 1;
  CHECK(!decode(bytes, other, decoded));
}
void validation() {
  Settings value = fixture(); uint8_t bytes[kSize];
  value.assembly = 0; CHECK(!encode(value, bytes)); value = fixture();
  value.generation = 0; CHECK(!encode(value, bytes)); value = fixture();
  memset(value.identity, 0, 16); CHECK(!encode(value, bytes)); value = fixture();
  value.known = 0x80; CHECK(!encode(value, bytes)); value = fixture();
  value.known = 1; value.negative = 2; CHECK(!encode(value, bytes));
  value.negative = 1; CHECK(encode(value, bytes));
  Store store; CHECK(save(store, value.identity, value, 0) == SaveStatus::Saved);
  const Loaded partial = load(store, value.identity); CHECK(partial.state == State::Partial);
  int32_t output = 999; CHECK(!orient_count(partial, 0, 10, 0, output)); CHECK(output == 999);
}
void journal() {
  Store store; const Settings value = fixture();
  CHECK(load(store, value.identity).state == State::Empty);
  CHECK(save(store, value.identity, value, 0) == SaveStatus::Saved);
  Loaded read = load(store, value.identity);
  CHECK(read.state == State::Usable && read.slot == 0 && read.value.generation == 1);
  const unsigned writes = store.calls;
  CHECK(save(store, value.identity, value, 1) == SaveStatus::Unchanged); CHECK(store.calls == writes);
  Settings changed = value; changed.negative ^= 1;
  CHECK(save(store, value.identity, changed, 0) == SaveStatus::Stale); CHECK(store.calls == writes);
  CHECK(save(store, value.identity, changed, 1) == SaveStatus::Saved);
  read = load(store, value.identity); CHECK(read.slot == 1 && read.value.generation == 2);
  CHECK(save(store, value.identity, value, 2) == SaveStatus::Saved);
  read = load(store, value.identity); CHECK(read.slot == 0 && read.value.generation == 3);
  for (uint16_t i = 0; i < kBase; ++i) CHECK(store.bytes[i] == 0xff);
  for (uint16_t i = kBase + 2 * kSize; i < 1024; ++i) CHECK(store.bytes[i] == 0xff);
}
void interruptions() {
  const Settings value = fixture(); Settings changed = value; changed.negative ^= 1;
  Store initial; CHECK(save(initial, value.identity, value, 0) == SaveStatus::Saved);
  for (int cut = 0; cut <= 65; ++cut) {
    Store store = initial; store.remaining = cut;
    const SaveStatus saved = save(store, value.identity, changed, 1);
    const Loaded after_reboot = load(store, value.identity);
    CHECK(after_reboot.state == State::Usable);
    if (cut < 65) { CHECK(saved == SaveStatus::WriteFailed); CHECK(after_reboot.value.generation == 1); CHECK(same_payload(after_reboot.value, value)); }
    else { CHECK(saved == SaveStatus::Saved); CHECK(after_reboot.value.generation == 2); CHECK(same_payload(after_reboot.value, changed)); }
    CHECK(memcmp(store.bytes, initial.bytes, kBase + kSize) == 0);
  }
  // Repeat all cut points while overwriting an older committed slot.
  CHECK(save(initial, value.identity, changed, 1) == SaveStatus::Saved);
  for (int cut = 0; cut <= 65; ++cut) {
    Store store = initial; store.remaining = cut;
    const SaveStatus saved = save(store, value.identity, value, 2);
    const Loaded after = load(store, value.identity); CHECK(after.state == State::Usable);
    CHECK(after.value.generation == (cut < 65 ? 2u : 3u));
    CHECK(saved == (cut < 65 ? SaveStatus::WriteFailed : SaveStatus::Saved));
    CHECK(memcmp(store.bytes + kBase + kSize, initial.bytes + kBase + kSize, kSize) == 0);
  }
}
void fail_closed() {
  Settings value = fixture(); Store store; place(store, 0, value);
  Settings other = value; other.generation = 2; other.negative ^= 1; place(store, 1, other);
  store.bytes[kBase + kSize + 33] ^= 1;
  Loaded read = load(store, value.identity); CHECK(read.state == State::Corrupt);
  int32_t output = 55; CHECK(!orient_count(read, 0, 10, 0, output)); CHECK(output == 55);
  CHECK(save(store, value.identity, value, 1) == SaveStatus::Unavailable); CHECK(store.calls == 0);
  place(store, 1, value); CHECK(load(store, value.identity).state == State::Ambiguous);
  Store exhausted; value.generation = UINT32_MAX; place(exhausted, 0, value);
  other = value; other.negative ^= 1;
  CHECK(save(exhausted, value.identity, other, UINT32_MAX) == SaveStatus::Exhausted); CHECK(exhausted.calls == 0);
  Store unknown_marker; place(unknown_marker,0,fixture()); unknown_marker.bytes[kBase+63]=0xa4;
  CHECK(load(unknown_marker, value.identity).state == State::Corrupt);
  unknown_marker.bytes[kBase+63]=0xff; CHECK(load(unknown_marker, value.identity).state == State::Corrupt);
  Store lost_ack; Settings original=fixture(); place(lost_ack,0,original); lost_ack.fail_after_commit=true;
  Settings revised=original; revised.negative ^= 1;
  CHECK(save(lost_ack,original.identity,revised,1)==SaveStatus::WriteFailed);
  CHECK(load(lost_ack,original.identity).value.generation==2);
  CHECK(save(lost_ack,original.identity,revised,1)==SaveStatus::Stale);
  Store dropped; dropped.drop = true;
  CHECK(save(dropped, value.identity, value, 0) == SaveStatus::WriteFailed);
  CHECK(load(dropped, value.identity).state == State::Empty);
}
void orientation() {
  const Settings value = fixture(); Store store; place(store, 0, value); Loaded loaded = load(store, value.identity);
  int32_t output = 0;
  CHECK(orient_count(loaded, 0, 123, 23, output) && output == 100);
  CHECK(orient_count(loaded, 1, 123, 23, output) && output == -100);
  CHECK(orient_count(loaded, 1, -8388608L, 8388607L, output) && output == 16777215L);
  output = 777; CHECK(!orient_count(loaded, 7, 10, 0, output)); CHECK(output == 777);
  CHECK(!orient_count(loaded, 0, INT32_MAX, 0, output)); CHECK(output == 777);
}
int main() {
  golden(); validation(); journal(); interruptions(); fail_closed(); orientation();
  puts("PASS: 6 device-setting groups; 132 interrupted-write/reboot cases; no hardware I/O");
  return 0;
}
