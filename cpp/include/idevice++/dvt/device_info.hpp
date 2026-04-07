// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/dvt/remote_server.hpp>
#include <idevice++/result.hpp>
#include <memory>
#include <string>
#include <vector>

namespace IdeviceFFI {

using DeviceInfoPtr =
    std::unique_ptr<DeviceInfoHandle, FnDeleter<DeviceInfoHandle, device_info_free>>;

/// A running process on the device
struct RunningProcess {
    uint32_t    pid;
    std::string name;
    std::string real_app_name;
    bool        is_application;
    uint64_t    start_page_count;
};

class DeviceInfo {
  public:
    static Result<DeviceInfo, FfiError> create(RemoteServer& server);

    Result<std::vector<RunningProcess>, FfiError> running_processes();
    Result<std::string, FfiError>                 execname_for_pid(uint32_t pid);
    Result<bool, FfiError>                        is_running_pid(uint32_t pid);
    /// Returns a plist_t dictionary. Caller must free with plist_free().
    Result<plist_t, FfiError>                     hardware_information();
    /// Returns a plist_t dictionary. Caller must free with plist_free().
    Result<plist_t, FfiError>                     network_information();
    Result<std::string, FfiError>                 mach_kernel_name();
    Result<std::vector<std::string>, FfiError>    sysmon_process_attributes();
    Result<std::vector<std::string>, FfiError>    sysmon_system_attributes();
    Result<std::vector<std::string>, FfiError>    directory_listing(const std::string& path);

    ~DeviceInfo() noexcept                         = default;
    DeviceInfo(DeviceInfo&&) noexcept              = default;
    DeviceInfo& operator=(DeviceInfo&&) noexcept   = default;
    DeviceInfo(const DeviceInfo&)                  = delete;
    DeviceInfo&       operator=(const DeviceInfo&) = delete;

    DeviceInfoHandle* raw() const noexcept { return handle_.get(); }
    static DeviceInfo adopt(DeviceInfoHandle* h) noexcept { return DeviceInfo(h); }

  private:
    explicit DeviceInfo(DeviceInfoHandle* h) noexcept : handle_(h) {}
    DeviceInfoPtr handle_{};

    static std::vector<std::string> collect_string_array(char** arr, size_t count);
};

} // namespace IdeviceFFI
