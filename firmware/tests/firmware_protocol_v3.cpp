// Native contract tests only. These helpers never access a serial port or device EEPROM.
#include "../shared/device_settings.h"
#include "../shared/buffered_line_queue.h"
#include "../shared/firmware_protocol_v3.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#define CHECK(x) do { if (!(x)) { fprintf(stderr, "FAIL line %d: %s\n", __LINE__, #x); exit(1); } } while (0)

using namespace selfrionette::protocol3;

void parser_contract() {
  Command command = {};
  char info[] = "info3,4294967295";
  CHECK(parse_command(info, command) == ParseStatus::Valid);
  CHECK(command.kind == CommandKind::Info && command.request == UINT32_MAX);
  char safe_info[] = "?3,4294967295";
  CHECK(is_v3_payload(safe_info));
  CHECK(parse_command(safe_info, command) == ParseStatus::Valid);
  CHECK(command.kind == CommandKind::Info && command.request == UINT32_MAX);
  char bad_alias[] = "?3,0";
  CHECK(parse_command(bad_alias, command) == ParseStatus::InvalidRequest);
  char extra_alias[] = "?3,1,extra";
  CHECK(parse_command(extra_alias, command) == ParseStatus::Malformed);

  char stream[] = "stream3,8,17,1";
  CHECK(parse_command(stream, command) == ParseStatus::Valid);
  CHECK(command.kind == CommandKind::Stream && command.request == 8 &&
        command.session == 17 && command.enabled);

  char stop[] = "stream3,9,17,0";
  CHECK(parse_command(stop, command) == ParseStatus::Valid && !command.enabled);

  char settings[] = "settings3,10,srn-0123456789abcdef0123456789abcdef,4,8,127,18";
  CHECK(parse_command(settings, command) == ParseStatus::Valid);
  CHECK(command.kind == CommandKind::Settings && command.expected_generation == 4 &&
        command.assembly == 8 && command.known == 127 && command.negative == 18 &&
        command.identity[0] == 0x01 && command.identity[15] == 0xef);

  char partial[] = "settings3,11,srn-0123456789abcdef0123456789abcdef,0,1,0,0";
  CHECK(parse_command(partial, command) == ParseStatus::Valid && command.known == 0);

  char longest_valid[] =
      "settings3,4294967295,srn-0123456789abcdef0123456789abcdef,"
      "4294967295,4294967295,127,127";
  CHECK(strlen(longest_valid) == kMaxValidCommandPayload);
  CHECK(parse_command(longest_valid, command) == ParseStatus::Valid);

  char tare[] = "tare3,12,srn-0123456789abcdef0123456789abcdef,4294967295";
  CHECK(parse_command(tare, command) == ParseStatus::Valid);
  CHECK(command.kind == CommandKind::Tare && command.expected_generation == UINT32_MAX);

  char bad_request[] = "info3,0";
  CHECK(parse_command(bad_request, command) == ParseStatus::InvalidRequest);
  char overflow_request[] = "info3,4294967296";
  CHECK(parse_command(overflow_request, command) == ParseStatus::InvalidRequest);
  char signed_number[] = "stream3,1,+2,1";
  CHECK(parse_command(signed_number, command) == ParseStatus::Malformed);
  char zero_session[] = "stream3,1,0,1";
  CHECK(parse_command(zero_session, command) == ParseStatus::Malformed);
  char invalid_enabled[] = "stream3,1,2,01";
  CHECK(parse_command(invalid_enabled, command) == ParseStatus::Malformed);
  char extra_field[] = "info3,1,extra";
  CHECK(parse_command(extra_field, command) == ParseStatus::Malformed);
  char bad_id[] = "tare3,1,srn-0123456789ABCDEF0123456789abcdef,0";
  CHECK(parse_command(bad_id, command) == ParseStatus::Malformed);
  char negative_unknown[] = "settings3,1,srn-0123456789abcdef0123456789abcdef,0,1,1,2";
  CHECK(parse_command(negative_unknown, command) == ParseStatus::Malformed);
  char mask_overflow[] = "settings3,1,srn-0123456789abcdef0123456789abcdef,0,1,128,0";
  CHECK(parse_command(mask_overflow, command) == ParseStatus::Malformed);
  char trailing[] = "tare3,1,srn-0123456789abcdef0123456789abcdef,0,";
  CHECK(parse_command(trailing, command) == ParseStatus::Malformed);
  char legacy[] = "info";
  CHECK(parse_command(legacy, command) == ParseStatus::NotV3);
  CHECK(kCommandPayloadMax == 87 && kMaxValidCommandPayload == 87 &&
        kFramePayloadMax == 511);
}

void quality_contract() {
  CHECK(flags_for(false, false, false, 0, false) == kTimeout);
  CHECK(flags_for(true, true, false, 10, false) == kFresh);
  CHECK(flags_for(true, true, false, -8388608L, false) == (kFresh | kSaturated));
  CHECK(flags_for(true, true, false, 10, true) == (kFresh | kRejected));
  CHECK(flags_for(true, false, false, 10, false) == kHeld);
  CHECK(flags_for(true, false, true, 10, false) == (kHeld | kTimeout));
  CHECK(eligible_fresh(kFresh));
  CHECK(!eligible_fresh(kFresh | kSaturated));
  CHECK(!eligible_fresh(kHeld));
  CHECK(deadline_elapsed(10, UINT32_MAX - 5U, 16));
  CHECK(!deadline_elapsed(9, UINT32_MAX - 5U, 16));
}

void sample_line_capacity_contract() {
  char line[256];
  const int length = snprintf(
      line, sizeof(line),
      "sample3,%lu,%lu,%lu,%lu,%lu,1"
      ",%ld,%ld,%ld,%ld,%ld,%ld,%ld"
      ",%ld,%ld,%ld,%ld,%ld,%ld,%ld"
      ",%u,%u,%u,%u,%u,%u,%u\n",
      static_cast<unsigned long>(UINT32_MAX),
      static_cast<unsigned long>(UINT32_MAX),
      static_cast<unsigned long>(UINT32_MAX),
      static_cast<unsigned long>(UINT32_MAX),
      static_cast<unsigned long>(UINT32_MAX),
      -8388608L, -8388608L, -8388608L, -8388608L, -8388608L, -8388608L,
      -8388608L, -16777215L, -16777215L, -16777215L, -16777215L,
      -16777215L, -16777215L, -16777215L,
      31U, 31U, 31U, 31U, 31U, 31U, 31U);
  CHECK(length == kMaxSample3LineBytes);
  CHECK(static_cast<size_t>(length) < sizeof(line));
  CHECK(line[length - 1] == '\n');
}

void tare_contract() {
  TareWindow window;
  for (uint8_t channel = 0; channel < kChannelCount; ++channel) {
    CHECK(!window.add(channel, 100, kHeld));
    CHECK(!window.add(channel, 8388607L, kFresh | kSaturated));
    for (uint8_t sample = 0; sample < kTareSamplesPerChannel; ++sample) {
      CHECK(window.add(channel, 100 + channel * 10 + sample, kFresh));
    }
  }
  CHECK(window.complete());
  int32_t zero[kChannelCount] = {};
  CHECK(window.finish(zero));
  for (uint8_t channel = 0; channel < kChannelCount; ++channel) {
    CHECK(zero[channel] == 103 + channel * 10);
  }

  TareWindow unstable;
  for (uint8_t channel = 0; channel < kChannelCount; ++channel) {
    for (uint8_t sample = 0; sample < kTareSamplesPerChannel; ++sample) {
      const int32_t raw = sample == 0 ? 0 : 2001;
      CHECK(unstable.add(channel, raw, kFresh));
    }
  }
  CHECK(unstable.complete() && !unstable.finish(zero));

  TareWindow incomplete;
  for (uint8_t channel = 0; channel + 1 < kChannelCount; ++channel) {
    for (uint8_t sample = 0; sample < kTareSamplesPerChannel; ++sample) {
      CHECK(incomplete.add(channel, 7, kFresh));
    }
  }
  CHECK(!incomplete.complete() && !incomplete.finish(zero));
}

void tare_deadline_contract() {
  const uint32_t started_ms = 1000;
  int32_t zero[kChannelCount] = {};

  TareWindow before_deadline;
  for (uint8_t channel = 0; channel < kChannelCount; ++channel) {
    for (uint8_t sample = 0; sample < kTareSamplesPerChannel; ++sample) {
      CHECK(before_deadline.add_before_deadline(
                started_ms + 1999, started_ms, channel, 100 + sample, kFresh) ==
            TareAdmission::Added);
    }
  }
  CHECK(before_deadline.finish_before_deadline(
            started_ms + 1999, started_ms, zero) == TareCompletion::Complete);

  for (uint32_t elapsed_ms = 2000; elapsed_ms <= 2001; ++elapsed_ms) {
    TareWindow at_or_after_deadline;
    for (uint8_t channel = 0; channel < kChannelCount; ++channel) {
      for (uint8_t sample = 0; sample < kTareSamplesPerChannel; ++sample) {
        const bool final_sample = channel == kChannelCount - 1 &&
                                  sample == kTareSamplesPerChannel - 1;
        const uint32_t sample_time = started_ms + (final_sample ? elapsed_ms : 1999);
        const TareAdmission admitted = at_or_after_deadline.add_before_deadline(
            sample_time, started_ms, channel, 100 + sample, kFresh);
        CHECK(admitted == (final_sample ? TareAdmission::TimedOut
                                        : TareAdmission::Added));
      }
    }
    CHECK(!at_or_after_deadline.complete());
    CHECK(at_or_after_deadline.count(kChannelCount - 1) ==
          kTareSamplesPerChannel - 1);
    CHECK(at_or_after_deadline.finish_before_deadline(
              started_ms + elapsed_ms, started_ms, zero) == TareCompletion::TimedOut);
  }

  TareWindow completed_before_deadline;
  for (uint8_t channel = 0; channel < kChannelCount; ++channel) {
    for (uint8_t sample = 0; sample < kTareSamplesPerChannel; ++sample) {
      CHECK(completed_before_deadline.add_before_deadline(
                started_ms + 1999, started_ms, channel, 100, kFresh) ==
            TareAdmission::Added);
    }
  }
  CHECK(completed_before_deadline.finish_before_deadline(
            started_ms + 2000, started_ms, zero) == TareCompletion::TimedOut);
}

struct MockSerialSink {
  int writable = 0;
  uint8_t output[256] = {};
  size_t length = 0;

  int availableForWrite() const { return writable; }

  size_t write(const uint8_t* data, size_t count) {
    size_t accepted = count;
    if (accepted > static_cast<size_t>(writable)) {
      accepted = static_cast<size_t>(writable);
    }
    if (accepted > sizeof(output) - length) accepted = sizeof(output) - length;
    memcpy(output + length, data, accepted);
    length += accepted;
    writable -= static_cast<int>(accepted);
    return accepted;
  }
};

void enqueue_blocked_line(selfrionette::firmware::BufferedLineQueue<256>& queue,
                          MockSerialSink& sink, uint16_t payload_bytes) {
  CHECK(queue.beginLine(37));
  for (uint16_t i = 0; i < payload_bytes; ++i) CHECK(queue.write('x') == 1);
  CHECK(queue.endLine());
  queue.drain(sink);
}

void buffered_output_queue_contract() {
  selfrionette::firmware::BufferedLineQueue<256> queue;
  MockSerialSink sink;

  enqueue_blocked_line(queue, sink, 53);
  enqueue_blocked_line(queue, sink, 53);
  enqueue_blocked_line(queue, sink, 54);
  enqueue_blocked_line(queue, sink, 54);
  CHECK(queue.busy());
  CHECK(sink.length == 0);

  // 218 bytes are queued, leaving one byte below the 37-byte reserve. This
  // failed begin must not leave the output queue in an active-line state.
  CHECK(!queue.beginLine(37));
  sink.writable = static_cast<int>(sizeof(sink.output));
  queue.drain(sink);
  CHECK(!queue.busy());
  CHECK(sink.length == 218);

  sink.writable = 0;
  CHECK(queue.beginLine(37));
  CHECK(queue.write('o') == 1);
  CHECK(queue.endLine());
  sink.writable = static_cast<int>(sizeof(sink.output) - sink.length);
  queue.drain(sink);
  CHECK(!queue.busy());
  CHECK(sink.length == 220 && sink.output[218] == 'o' && sink.output[219] == '\n');
}

void tare_then_same_session_resume_contract() {
  StreamSession session;
  CHECK(session.start(17) == SessionStart::NewSession);
  uint32_t sequence = 0;
  CHECK(session.advanceSequence(sequence) && sequence == 1);
  CHECK(session.stop(17));

  TareWindow window;
  for (uint8_t channel = 0; channel < kChannelCount; ++channel) {
    for (uint8_t sample = 0; sample < kTareSamplesPerChannel; ++sample) {
      CHECK(window.add(channel, 100 + channel * 10 + sample, kFresh));
    }
  }
  int32_t zero[kChannelCount] = {};
  CHECK(window.complete() && window.finish(zero));
  const bool tare_valid = true;

  CHECK(session.start(17) == SessionStart::Resumed);
  CHECK(session.active() && session.challenge() == 17 && session.sequence() == 1);
  CHECK(session.advanceSequence(sequence) && sequence == 2);

  selfrionette::settings::Loaded loaded = {};
  loaded.state = selfrionette::settings::State::Usable;
  loaded.value.generation = 1;
  loaded.value.assembly = 1;
  loaded.value.identity[0] = 1;
  loaded.value.known = selfrionette::settings::kAllKnown;
  for (uint8_t channel = 0; channel < kChannelCount; ++channel) {
    const int32_t raw = zero[channel] + 5;
    CHECK(flags_for(true, true, false, raw, false) == kFresh);
    int32_t oriented = 0;
    CHECK(tare_valid && selfrionette::settings::orient_count(
        loaded, channel, raw, zero[channel], oriented));
    CHECK(oriented == 5);
  }

  CHECK(!session.stop(18));
  CHECK(session.start(18) == SessionStart::NewSession);
  CHECK(session.sequence() == 0);
}

void orientation_contract() {
  using namespace selfrionette::settings;
  Loaded loaded = {};
  loaded.state = State::Usable;
  loaded.value.generation = 1;
  loaded.value.assembly = 1;
  loaded.value.identity[0] = 1;
  loaded.value.known = kAllKnown;
  loaded.value.negative = 0x02;

  int32_t oriented = 0;
  CHECK(orient_count(loaded, 0, 123, 23, oriented) && oriented == 100);
  CHECK(orient_count(loaded, 1, 123, 23, oriented) && oriented == -100);
  CHECK(orient_count(loaded, 1, -8388608L, 8388607L, oriented) && oriented == 16777215L);

  loaded.state = State::Partial;
  oriented = 99;
  CHECK(!orient_count(loaded, 0, 123, 23, oriented) && oriented == 99);
}

int main() {
  parser_contract();
  quality_contract();
  sample_line_capacity_contract();
  tare_contract();
  tare_deadline_contract();
  buffered_output_queue_contract();
  tare_then_same_session_resume_contract();
  orientation_contract();
  puts("PASS: protocol v3 parser, quality, tare, resume, and signed orientation contracts");
  return 0;
}
