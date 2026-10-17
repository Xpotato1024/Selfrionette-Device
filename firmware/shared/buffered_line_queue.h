#ifndef SELFRIONETTE_BUFFERED_LINE_QUEUE_H
#define SELFRIONETTE_BUFFERED_LINE_QUEUE_H

#include <stddef.h>
#include <stdint.h>
#include <string.h>

namespace selfrionette {
namespace firmware {

template <uint16_t Capacity>
class BufferedLineQueue {
 public:
  bool beginLine(uint16_t reserve = 0) {
    if (line_active_) return false;
    compact();
    if (reserve > Capacity) return false;

    const uint16_t limit = static_cast<uint16_t>(Capacity - reserve);
    if (limit < length_ || limit - length_ < 2) return false;

    line_start_ = length_;
    line_limit_ = limit;
    line_accepted_ = true;
    overflow_ = false;
    line_active_ = true;
    return true;
  }

  bool endLine() {
    if (!line_active_) return false;
    const bool accepted = line_accepted_ && !overflow_ && length_ < line_limit_;
    if (accepted) buffer_[length_++] = '\n';
    else length_ = line_start_;
    line_active_ = false;
    line_accepted_ = false;
    return accepted;
  }

  bool busy() const { return sent_ < length_; }

  bool canQueue(uint16_t required) {
    if (line_active_) return false;
    compact();
    return required <= Capacity - length_;
  }

  template <typename Sink>
  void drain(Sink& sink) {
    while (busy()) {
      const int available = sink.availableForWrite();
      if (available <= 0) return;
      const uint16_t remaining = static_cast<uint16_t>(length_ - sent_);
      const uint16_t writable = static_cast<uint16_t>(available);
      const uint16_t count = static_cast<uint16_t>(
          writable < remaining ? writable : remaining);
      const size_t written = sink.write(
          reinterpret_cast<const uint8_t*>(buffer_ + sent_), count);
      if (written == 0) return;
      sent_ = static_cast<uint16_t>(sent_ + written);
    }
    length_ = sent_ = 0;
  }

  size_t write(uint8_t value) {
    if (!line_active_ || !line_accepted_) return 0;
    if (length_ >= line_limit_ - 1) {
      overflow_ = true;
      line_accepted_ = false;
      return 0;
    }
    buffer_[length_++] = static_cast<char>(value);
    return 1;
  }

 private:
  void compact() {
    if (sent_ == 0) return;
    const uint16_t remaining = static_cast<uint16_t>(length_ - sent_);
    memmove(buffer_, buffer_ + sent_, remaining);
    length_ = remaining;
    sent_ = 0;
  }

  char buffer_[Capacity];
  uint16_t length_ = 0;
  uint16_t sent_ = 0;
  uint16_t line_start_ = 0;
  uint16_t line_limit_ = 0;
  bool line_active_ = false;
  bool line_accepted_ = false;
  bool overflow_ = false;
};

}  // namespace firmware
}  // namespace selfrionette

#endif
