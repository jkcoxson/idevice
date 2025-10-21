// Jackson Coxson

#pragma once
#include <cstring>
#include <idevice++/bindings.hpp>
#include <idevice++/dvt/remote_server.hpp>
#include <idevice++/result.hpp>
#include <memory>
#include <vector>

namespace IdeviceFFI {

using ScreenshotClientPtr =
    std::unique_ptr<ScreenshotClientHandle,
                    FnDeleter<ScreenshotClientHandle, screenshot_client_free>>;

/// C++ wrapper around the ScreenshotClient FFI handle
///
/// Provides a high-level interface for capturing screenshots
/// from a connected iOS device through the DVT service.
class ScreenshotClient {
  public:
    /// Creates a new ScreenshotClient using an existing RemoteServer.
    ///
    /// The RemoteServer is borrowed, not consumed.
    static Result<ScreenshotClient, FfiError> create(RemoteServer& server);

    /// Captures a screenshot and returns it as a PNG buffer.
    ///
    /// On success, returns a vector containing PNG-encoded bytes.
    Result<std::vector<uint8_t>, FfiError>    take_screenshot();

    ~ScreenshotClient() noexcept                               = default;
    ScreenshotClient(ScreenshotClient&&) noexcept              = default;
    ScreenshotClient& operator=(ScreenshotClient&&) noexcept   = default;
    ScreenshotClient(const ScreenshotClient&)                  = delete;
    ScreenshotClient&       operator=(const ScreenshotClient&) = delete;

    ScreenshotClientHandle* raw() const noexcept { return handle_.get(); }
    static ScreenshotClient adopt(ScreenshotClientHandle* h) noexcept {
        return ScreenshotClient(h);
    }

  private:
    explicit ScreenshotClient(ScreenshotClientHandle* h) noexcept : handle_(h) {}
    ScreenshotClientPtr handle_{};
};

} // namespace IdeviceFFI
