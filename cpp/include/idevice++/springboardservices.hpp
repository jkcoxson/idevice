// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <string>
#include <vector>

namespace IdeviceFFI {

using SpringBoardServicesPtr =
    std::unique_ptr<SpringBoardServicesClientHandle,
                    FnDeleter<SpringBoardServicesClientHandle, springboard_services_free>>;

class SpringBoardServices {
  public:
    // Factory: connect via Provider
    static Result<SpringBoardServices, FfiError> connect(Provider& provider);

    // Factory: wrap an existing Idevice socket (consumes it on success)
    static Result<SpringBoardServices, FfiError> from_socket(Idevice&& socket);

    // Ops
    Result<std::vector<uint8_t>, FfiError>       get_icon(const std::string& bundle_identifier);
    Result<std::vector<uint8_t>, FfiError>       get_home_screen_wallpaper_preview();
    Result<std::vector<uint8_t>, FfiError>       get_lock_screen_wallpaper_preview();
    Result<uint8_t, FfiError>                    get_interface_orientation();
    Result<plist_t, FfiError>                    get_homescreen_icon_metrics();

    // RAII / moves
    ~SpringBoardServices() noexcept                                        = default;
    SpringBoardServices(SpringBoardServices&&) noexcept                    = default;
    SpringBoardServices& operator=(SpringBoardServices&&) noexcept         = default;
    SpringBoardServices(const SpringBoardServices&)                        = delete;
    SpringBoardServices&             operator=(const SpringBoardServices&) = delete;

    SpringBoardServicesClientHandle* raw() const noexcept { return handle_.get(); }
    static SpringBoardServices       adopt(SpringBoardServicesClientHandle* h) noexcept {
        return SpringBoardServices(h);
    }

  private:
    explicit SpringBoardServices(SpringBoardServicesClientHandle* h) noexcept : handle_(h) {}
    SpringBoardServicesPtr handle_{};
};

} // namespace IdeviceFFI
