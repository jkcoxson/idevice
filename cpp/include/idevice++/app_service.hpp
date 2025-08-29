// Jackson Coxson

#pragma once
#include <cstddef>
#include <cstdint>
#include <idevice++/adapter_stream.hpp>
#include <idevice++/core_device_proxy.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/readwrite.hpp>
#include <idevice++/rsd.hpp>
#include <memory>
#include <string>
#include <vector>

namespace IdeviceFFI {

using AppServicePtr =
    std::unique_ptr<AppServiceHandle, FnDeleter<AppServiceHandle, app_service_free>>;

struct AppInfo {
    bool                is_removable{};
    std::string         name;
    bool                is_first_party{};
    std::string         path;
    std::string         bundle_identifier;
    bool                is_developer_app{};
    Option<std::string> bundle_version;
    bool                is_internal{};
    bool                is_hidden{};
    bool                is_app_clip{};
    Option<std::string> version;
};

struct LaunchResponse {
    uint32_t              process_identifier_version{};
    uint32_t              pid{};
    std::string           executable_url;
    std::vector<uint32_t> audit_token; // raw words
};

struct ProcessToken {
    uint32_t            pid{};
    Option<std::string> executable_url;
};

struct SignalResponse {
    uint32_t            pid{};
    Option<std::string> executable_url;
    uint64_t            device_timestamp_ms{};
    uint32_t            signal{};
};

struct IconData {
    std::vector<uint8_t> data;
    double               icon_width{};
    double               icon_height{};
    double               minimum_width{};
    double               minimum_height{};
};

class AppService {
  public:
    // Factory: connect via RSD (borrows adapter & handshake)
    static Result<AppService, FfiError> connect_rsd(Adapter& adapter, RsdHandshake& rsd);

    // Factory: from socket Box<dyn ReadWrite> (consumes it).
    static Result<AppService, FfiError> from_readwrite_ptr(ReadWriteOpaque* consumed);

    // nice ergonomic overload: consume a C++ ReadWrite by releasing it
    static Result<AppService, FfiError> from_readwrite(ReadWrite&& rw);

    // API
    Result<std::vector<AppInfo>, FfiError>
    list_apps(bool app_clips, bool removable, bool hidden, bool internal, bool default_apps) const;

    Result<LaunchResponse, FfiError>            launch(const std::string&              bundle_id,
                                                       const std::vector<std::string>& argv,
                                                       bool                            kill_existing,
                                                       bool                            start_suspended);

    Result<std::vector<ProcessToken>, FfiError> list_processes() const;

    Result<void, FfiError>                      uninstall(const std::string& bundle_id);

    Result<SignalResponse, FfiError>            send_signal(uint32_t pid, uint32_t signal);

    Result<IconData, FfiError>                  fetch_icon(const std::string& bundle_id,
                                                           float              width,
                                                           float              height,
                                                           float              scale,
                                                           bool               allow_placeholder);

    // RAII / moves
    ~AppService() noexcept                         = default;
    AppService(AppService&&) noexcept              = default;
    AppService& operator=(AppService&&) noexcept   = default;
    AppService(const AppService&)                  = delete;
    AppService&       operator=(const AppService&) = delete;

    AppServiceHandle* raw() const noexcept { return handle_.get(); }
    static AppService adopt(AppServiceHandle* h) noexcept { return AppService(h); }

  private:
    explicit AppService(AppServiceHandle* h) noexcept : handle_(h) {}
    AppServicePtr handle_{};
};

} // namespace IdeviceFFI
