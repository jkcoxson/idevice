// Jackson Coxson

#pragma once
#include <cstddef>
#include <cstdint>
#include <string>
#include <vector>

#include <idevice++/bindings.hpp>
#include <idevice++/core_device_proxy.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/rsd.hpp>

namespace IdeviceFFI {

class SysdiagnoseStream {
  public:
    SysdiagnoseStream()                                    = default;
    SysdiagnoseStream(const SysdiagnoseStream&)            = delete;
    SysdiagnoseStream& operator=(const SysdiagnoseStream&) = delete;

    SysdiagnoseStream(SysdiagnoseStream&& other) noexcept : h_(other.h_) { other.h_ = nullptr; }
    SysdiagnoseStream& operator=(SysdiagnoseStream&& other) noexcept {
        if (this != &other) {
            reset();
            h_       = other.h_;
            other.h_ = nullptr;
        }
        return *this;
    }

    ~SysdiagnoseStream() { reset(); }

    // Pull next chunk. Returns nullopt on end-of-stream. On error, returns
    // nullopt and sets `err`.
    Result<Option<std::vector<uint8_t>>, FfiError> next_chunk();

    SysdiagnoseStreamHandle*                       raw() const { return h_; }

  private:
    friend class DiagnosticsService;
    explicit SysdiagnoseStream(::SysdiagnoseStreamHandle* h) : h_(h) {}

    void reset() {
        if (h_) {
            ::sysdiagnose_stream_free(h_);
            h_ = nullptr;
        }
    }

    ::SysdiagnoseStreamHandle* h_ = nullptr;
};

// The result of starting a sysdiagnose capture.
struct SysdiagnoseCapture {
    std::string       preferred_filename;
    std::size_t       expected_length = 0;
    SysdiagnoseStream stream;
};

// RAII for Diagnostics service client
class DiagnosticsService {
  public:
    DiagnosticsService()                                     = default;
    DiagnosticsService(const DiagnosticsService&)            = delete;
    DiagnosticsService& operator=(const DiagnosticsService&) = delete;

    DiagnosticsService(DiagnosticsService&& other) noexcept : h_(other.h_) { other.h_ = nullptr; }
    DiagnosticsService& operator=(DiagnosticsService&& other) noexcept {
        if (this != &other) {
            reset();
            h_       = other.h_;
            other.h_ = nullptr;
        }
        return *this;
    }

    ~DiagnosticsService() { reset(); }

    // Connect via RSD (borrows adapter & handshake; does not consume them)
    static Result<DiagnosticsService, FfiError> connect_rsd(Adapter& adapter, RsdHandshake& rsd);

    // Create from a ReadWrite stream (consumes it)
    static Result<DiagnosticsService, FfiError> from_stream_ptr(::ReadWriteOpaque* consumed);

    static Result<DiagnosticsService, FfiError> from_stream(ReadWrite&& rw) {
        return from_stream_ptr(rw.release());
    }

    // Start sysdiagnose capture; on success returns filename, length and a byte
    // stream
    Result<SysdiagnoseCapture, FfiError> capture_sysdiagnose(bool dry_run);

    ::DiagnosticsServiceHandle*          raw() const { return h_; }

  private:
    explicit DiagnosticsService(::DiagnosticsServiceHandle* h) : h_(h) {}

    void reset() {
        if (h_) {
            ::diagnostics_service_free(h_);
            h_ = nullptr;
        }
    }

    ::DiagnosticsServiceHandle* h_ = nullptr;
};

} // namespace IdeviceFFI
