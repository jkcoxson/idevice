// Jackson Coxson

#include <cstring>
#include <idevice++/bindings.hpp>
#include <idevice++/crashreportcopymobile.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>

namespace IdeviceFFI {

// -------- Factory Methods --------

Result<CrashReportCopyMobile, FfiError> CrashReportCopyMobile::connect(Provider& provider) {
    CrashReportCopyMobileHandle* out = nullptr;
    FfiError                     e(::crash_report_client_connect(provider.raw(), &out));
    if (e) {
        return Err(e);
    }
    return Ok(CrashReportCopyMobile::adopt(out));
}

Result<CrashReportCopyMobile, FfiError> CrashReportCopyMobile::from_socket(Idevice&& socket) {
    CrashReportCopyMobileHandle* out = nullptr;
    FfiError                     e(::crash_report_client_new(socket.raw(), &out));
    if (e) {
        return Err(e);
    }
    socket.release();
    return Ok(CrashReportCopyMobile::adopt(out));
}

Result<void, FfiError> CrashReportCopyMobile::flush(Provider& provider) {
    FfiError e(::crash_report_flush(provider.raw()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

// -------- Ops --------

Result<std::vector<std::string>, FfiError>
CrashReportCopyMobile::ls(const char* dir_path) {
    char** entries_raw = nullptr;
    size_t count       = 0;

    FfiError e(::crash_report_client_ls(handle_.get(), dir_path, &entries_raw, &count));
    if (e) {
        return Err(e);
    }

    std::vector<std::string> result;
    if (entries_raw) {
        result.reserve(count);
        for (size_t i = 0; i < count; ++i) {
            if (entries_raw[i]) {
                result.emplace_back(entries_raw[i]);
                ::idevice_string_free(entries_raw[i]);
            }
        }
        std::free(entries_raw);
    }

    return Ok(std::move(result));
}

Result<std::vector<char>, FfiError>
CrashReportCopyMobile::pull(const std::string& log_name) {
    uint8_t* data   = nullptr;
    size_t   length = 0;

    FfiError e(::crash_report_client_pull(handle_.get(), log_name.c_str(), &data, &length));
    if (e) {
        return Err(e);
    }

    std::vector<char> result;
    if (data && length > 0) {
        result.assign(reinterpret_cast<char*>(data), reinterpret_cast<char*>(data) + length);
        ::idevice_data_free(data, length);
    }

    return Ok(std::move(result));
}

Result<void, FfiError> CrashReportCopyMobile::remove(const std::string& log_name) {
    FfiError e(::crash_report_client_remove(handle_.get(), log_name.c_str()));
    if (e) {
        return Err(e);
    }
    return Ok();
}

} // namespace IdeviceFFI
