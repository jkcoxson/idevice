// Jackson Coxson

#pragma once
#include <cstddef>
#include <cstdint>
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <vector>

struct IdeviceFfiError;
struct AdapterStreamHandle;

namespace IdeviceFFI {

// Non-owning view over a stream (must call close(); no implicit free provided)
class AdapterStream {
  public:
    explicit AdapterStream(AdapterStreamHandle* h) noexcept : h_(h) {}

    AdapterStream(const AdapterStream&)            = delete;
    AdapterStream& operator=(const AdapterStream&) = delete;

    AdapterStream(AdapterStream&& other) noexcept : h_(other.h_) { other.h_ = nullptr; }
    AdapterStream& operator=(AdapterStream&& other) noexcept {
        if (this != &other) {
            h_       = other.h_;
            other.h_ = nullptr;
        }
        return *this;
    }

    ~AdapterStream() noexcept = default; // no auto-close; caller controls

    AdapterStreamHandle* raw() const noexcept { return h_; }

    bool                 close(FfiError& err);
    bool                 send(const uint8_t* data, size_t len, FfiError& err);
    bool                 send(const std::vector<uint8_t>& buf, FfiError& err) {
        return send(buf.data(), buf.size(), err);
    }

    // recv into caller-provided buffer (resizes to actual length)
    bool recv(std::vector<uint8_t>& out, FfiError& err, size_t max_hint = 2048);

  private:
    AdapterStreamHandle* h_{};
};

} // namespace IdeviceFFI
