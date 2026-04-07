// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <vector>

namespace IdeviceFFI {

using ScreenshotrPtr = std::unique_ptr<ScreenshotrClientHandle,
                                       FnDeleter<ScreenshotrClientHandle, screenshotr_client_free>>;

class Screenshotr {
  public:
    // Factory: connect via Provider
    static Result<Screenshotr, FfiError>   connect(Provider& provider);

    // Factory: connect via RSD tunnel
    static Result<Screenshotr, FfiError>   connect_rsd(AdapterHandle* adapter, RsdHandshakeHandle* handshake);

    // Ops
    Result<std::vector<uint8_t>, FfiError> take_screenshot();

    // RAII / moves
    ~Screenshotr() noexcept                                = default;
    Screenshotr(Screenshotr&&) noexcept                    = default;
    Screenshotr& operator=(Screenshotr&&) noexcept         = default;
    Screenshotr(const Screenshotr&)                        = delete;
    Screenshotr&             operator=(const Screenshotr&) = delete;

    ScreenshotrClientHandle* raw() const noexcept { return handle_.get(); }
    static Screenshotr       adopt(ScreenshotrClientHandle* h) noexcept { return Screenshotr(h); }

  private:
    explicit Screenshotr(ScreenshotrClientHandle* h) noexcept : handle_(h) {}
    ScreenshotrPtr handle_{};
};

} // namespace IdeviceFFI
