// Jackson Coxson

#include <idevice++/app_service.hpp>

namespace IdeviceFFI {

// ---- Factories ----
std::optional<AppService>
AppService::connect_rsd(Adapter& adapter, RsdHandshake& rsd, FfiError& err) {
    AppServiceHandle* out = nullptr;
    if (IdeviceFfiError* e = ::app_service_connect_rsd(adapter.raw(), rsd.raw(), &out)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return AppService::adopt(out);
}

std::optional<AppService> AppService::from_readwrite_ptr(ReadWriteOpaque* consumed, FfiError& err) {
    AppServiceHandle* out = nullptr;
    if (IdeviceFfiError* e = ::app_service_new(consumed, &out)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return AppService::adopt(out);
}

std::optional<AppService> AppService::from_readwrite(ReadWrite&& rw, FfiError& err) {
    // Rust consumes the stream regardless of result → release BEFORE call
    return from_readwrite_ptr(rw.release(), err);
}

// ---- Helpers to copy/free C arrays ----
static std::vector<AppInfo> copy_and_free_app_list(AppListEntryC* arr, size_t n) {
    std::vector<AppInfo> out;
    out.reserve(n);
    for (size_t i = 0; i < n; ++i) {
        const auto& c = arr[i];
        AppInfo     a;
        a.is_removable = c.is_removable != 0;
        if (c.name)
            a.name = c.name;
        a.is_first_party = c.is_first_party != 0;
        if (c.path)
            a.path = c.path;
        if (c.bundle_identifier)
            a.bundle_identifier = c.bundle_identifier;
        a.is_developer_app = c.is_developer_app != 0;
        if (c.bundle_version)
            a.bundle_version = std::string(c.bundle_version);
        a.is_internal = c.is_internal != 0;
        a.is_hidden   = c.is_hidden != 0;
        a.is_app_clip = c.is_app_clip != 0;
        if (c.version)
            a.version = std::string(c.version);
        out.emplace_back(std::move(a));
    }
    ::app_service_free_app_list(arr, n);
    return out;
}

static std::vector<ProcessToken> copy_and_free_process_list(ProcessTokenC* arr, size_t n) {
    std::vector<ProcessToken> out;
    out.reserve(n);
    for (size_t i = 0; i < n; ++i) {
        ProcessToken p;
        p.pid = arr[i].pid;
        if (arr[i].executable_url)
            p.executable_url = std::string(arr[i].executable_url);
        out.emplace_back(std::move(p));
    }
    ::app_service_free_process_list(arr, n);
    return out;
}

// ---- API impls ----
std::optional<std::vector<AppInfo>> AppService::list_apps(bool      app_clips,
                                                          bool      removable,
                                                          bool      hidden,
                                                          bool      internal,
                                                          bool      default_apps,
                                                          FfiError& err) const {
    AppListEntryC* arr = nullptr;
    size_t         n   = 0;
    if (IdeviceFfiError* e = ::app_service_list_apps(handle_.get(),
                                                     app_clips ? 1 : 0,
                                                     removable ? 1 : 0,
                                                     hidden ? 1 : 0,
                                                     internal ? 1 : 0,
                                                     default_apps ? 1 : 0,
                                                     &arr,
                                                     &n)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return copy_and_free_app_list(arr, n);
}

std::optional<LaunchResponse> AppService::launch(const std::string&              bundle_id,
                                                 const std::vector<std::string>& argv,
                                                 bool                            kill_existing,
                                                 bool                            start_suspended,
                                                 FfiError&                       err) {
    std::vector<const char*> c_argv;
    c_argv.reserve(argv.size());
    for (auto& s : argv)
        c_argv.push_back(s.c_str());

    LaunchResponseC* resp = nullptr;
    if (IdeviceFfiError* e = ::app_service_launch_app(handle_.get(),
                                                      bundle_id.c_str(),
                                                      c_argv.empty() ? nullptr : c_argv.data(),
                                                      c_argv.size(),
                                                      kill_existing ? 1 : 0,
                                                      start_suspended ? 1 : 0,
                                                      &resp)) {
        err = FfiError(e);
        return std::nullopt;
    }

    LaunchResponse out;
    out.process_identifier_version = resp->process_identifier_version;
    out.pid                        = resp->pid;
    if (resp->executable_url)
        out.executable_url = resp->executable_url;
    if (resp->audit_token && resp->audit_token_len > 0) {
        out.audit_token.assign(resp->audit_token, resp->audit_token + resp->audit_token_len);
    }
    ::app_service_free_launch_response(resp);
    return out;
}

std::optional<std::vector<ProcessToken>> AppService::list_processes(FfiError& err) const {
    ProcessTokenC* arr = nullptr;
    size_t         n   = 0;
    if (IdeviceFfiError* e = ::app_service_list_processes(handle_.get(), &arr, &n)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return copy_and_free_process_list(arr, n);
}

bool AppService::uninstall(const std::string& bundle_id, FfiError& err) {
    if (IdeviceFfiError* e = ::app_service_uninstall_app(handle_.get(), bundle_id.c_str())) {
        err = FfiError(e);
        return false;
    }
    return true;
}

std::optional<SignalResponse>
AppService::send_signal(uint32_t pid, uint32_t signal, FfiError& err) {
    SignalResponseC* c = nullptr;
    if (IdeviceFfiError* e = ::app_service_send_signal(handle_.get(), pid, signal, &c)) {
        err = FfiError(e);
        return std::nullopt;
    }
    SignalResponse out;
    out.pid = c->pid;
    if (c->executable_url)
        out.executable_url = std::string(c->executable_url);
    out.device_timestamp_ms = c->device_timestamp;
    out.signal              = c->signal;
    ::app_service_free_signal_response(c);
    return out;
}

std::optional<IconData> AppService::fetch_icon(const std::string& bundle_id,
                                               float              width,
                                               float              height,
                                               float              scale,
                                               bool               allow_placeholder,
                                               FfiError&          err) {
    IconDataC* c = nullptr;
    if (IdeviceFfiError* e = ::app_service_fetch_app_icon(handle_.get(),
                                                          bundle_id.c_str(),
                                                          width,
                                                          height,
                                                          scale,
                                                          allow_placeholder ? 1 : 0,
                                                          &c)) {
        err = FfiError(e);
        return std::nullopt;
    }
    IconData out;
    if (c->data && c->data_len) {
        out.data.assign(c->data, c->data + c->data_len);
    }
    out.icon_width     = c->icon_width;
    out.icon_height    = c->icon_height;
    out.minimum_width  = c->minimum_width;
    out.minimum_height = c->minimum_height;
    ::app_service_free_icon_data(c);
    return out;
}

} // namespace IdeviceFFI
