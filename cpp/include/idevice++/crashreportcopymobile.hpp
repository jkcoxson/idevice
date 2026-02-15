// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>
#include <string>
#include <vector>

namespace IdeviceFFI {

using CrashReportCopyMobilePtr =
    std::unique_ptr<CrashReportCopyMobileHandle,
                    FnDeleter<CrashReportCopyMobileHandle, crash_report_client_free>>;

class CrashReportCopyMobile {
  public:
    // Factory: connect via Provider
    static Result<CrashReportCopyMobile, FfiError> connect(Provider& provider);

    // Factory: wrap an existing Idevice socket (consumes it on success)
    static Result<CrashReportCopyMobile, FfiError> from_socket(Idevice&& socket);

    // Static: flush crash reports from system storage
    static Result<void, FfiError>                  flush(Provider& provider);

    // Ops
    Result<std::vector<std::string>, FfiError>     ls(const char* dir_path = nullptr);
    Result<std::vector<char>, FfiError>            pull(const std::string& log_name);
    Result<void, FfiError>                         remove(const std::string& log_name);

    // RAII / moves
    ~CrashReportCopyMobile() noexcept                                          = default;
    CrashReportCopyMobile(CrashReportCopyMobile&&) noexcept                    = default;
    CrashReportCopyMobile& operator=(CrashReportCopyMobile&&) noexcept         = default;
    CrashReportCopyMobile(const CrashReportCopyMobile&)                        = delete;
    CrashReportCopyMobile& operator=(const CrashReportCopyMobile&)             = delete;

    CrashReportCopyMobileHandle* raw() const noexcept { return handle_.get(); }
    static CrashReportCopyMobile adopt(CrashReportCopyMobileHandle* h) noexcept {
        return CrashReportCopyMobile(h);
    }

  private:
    explicit CrashReportCopyMobile(CrashReportCopyMobileHandle* h) noexcept : handle_(h) {}
    CrashReportCopyMobilePtr handle_{};
};

} // namespace IdeviceFFI
