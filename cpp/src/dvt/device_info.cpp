// Jackson Coxson

#include <idevice++/dvt/device_info.hpp>

namespace IdeviceFFI {

Result<DeviceInfo, FfiError> DeviceInfo::create(RemoteServer& server) {
    DeviceInfoHandle* out = nullptr;
    FfiError          e(::device_info_new(server.raw(), &out));
    if (e) return Err(e);
    return Ok(DeviceInfo::adopt(out));
}

std::vector<std::string> DeviceInfo::collect_string_array(char** arr, size_t count) {
    std::vector<std::string> result;
    result.reserve(count);
    for (size_t i = 0; i < count; ++i) {
        if (arr[i]) result.emplace_back(arr[i]);
    }
    ::device_info_string_array_free(arr, count);
    return result;
}

Result<std::vector<RunningProcess>, FfiError> DeviceInfo::running_processes() {
    IdeviceRunningProcess** ptrs  = nullptr;
    size_t                  count = 0;
    FfiError                e(::device_info_running_processes(handle_.get(), &ptrs, &count));
    if (e) return Err(e);

    std::vector<RunningProcess> result;
    result.reserve(count);
    for (size_t i = 0; i < count; ++i) {
        auto* p = ptrs[i];
        result.push_back({
            p->pid,
            p->name           ? std::string(p->name)           : std::string(),
            p->real_app_name  ? std::string(p->real_app_name)  : std::string(),
            p->is_application,
            p->start_page_count,
        });
    }
    ::device_info_running_processes_free(ptrs, count);
    return Ok(std::move(result));
}

Result<std::string, FfiError> DeviceInfo::execname_for_pid(uint32_t pid) {
    char*    out = nullptr;
    FfiError e(::device_info_execname_for_pid(handle_.get(), pid, &out));
    if (e) return Err(e);
    std::string s = out ? std::string(out) : std::string();
    ::idevice_string_free(out);
    return Ok(std::move(s));
}

Result<bool, FfiError> DeviceInfo::is_running_pid(uint32_t pid) {
    bool     result = false;
    FfiError e(::device_info_is_running_pid(handle_.get(), pid, &result));
    if (e) return Err(e);
    return Ok(result);
}

Result<plist_t, FfiError> DeviceInfo::hardware_information() {
    plist_t  out = nullptr;
    FfiError e(::device_info_hardware_information(handle_.get(), &out));
    if (e) return Err(e);
    return Ok(out);
}

Result<plist_t, FfiError> DeviceInfo::network_information() {
    plist_t  out = nullptr;
    FfiError e(::device_info_network_information(handle_.get(), &out));
    if (e) return Err(e);
    return Ok(out);
}

Result<std::string, FfiError> DeviceInfo::mach_kernel_name() {
    char*    out = nullptr;
    FfiError e(::device_info_mach_kernel_name(handle_.get(), &out));
    if (e) return Err(e);
    std::string s = out ? std::string(out) : std::string();
    ::idevice_string_free(out);
    return Ok(std::move(s));
}

Result<std::vector<std::string>, FfiError> DeviceInfo::sysmon_process_attributes() {
    char** attrs = nullptr;
    size_t count = 0;
    FfiError e(::device_info_sysmon_process_attributes(handle_.get(), &attrs, &count));
    if (e) return Err(e);
    return Ok(collect_string_array(attrs, count));
}

Result<std::vector<std::string>, FfiError> DeviceInfo::sysmon_system_attributes() {
    char** attrs = nullptr;
    size_t count = 0;
    FfiError e(::device_info_sysmon_system_attributes(handle_.get(), &attrs, &count));
    if (e) return Err(e);
    return Ok(collect_string_array(attrs, count));
}

Result<std::vector<std::string>, FfiError> DeviceInfo::directory_listing(
    const std::string& path) {
    char** entries = nullptr;
    size_t count   = 0;
    FfiError e(::device_info_directory_listing(handle_.get(), path.c_str(), &entries, &count));
    if (e) return Err(e);
    return Ok(collect_string_array(entries, count));
}

} // namespace IdeviceFFI
