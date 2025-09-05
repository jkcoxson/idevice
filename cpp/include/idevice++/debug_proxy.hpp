// Jackson Coxson

#pragma once
#include <cstddef>
#include <cstdint>
#include <memory>
#include <string>
#include <vector>

// Bring in the global C ABI (all C structs/functions are global)
#include <idevice++/core_device_proxy.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/result.hpp>
#include <idevice++/rsd.hpp>

namespace IdeviceFFI {

class DebugProxy {
  public:
    DebugProxy()                             = default;
    DebugProxy(const DebugProxy&)            = delete;
    DebugProxy& operator=(const DebugProxy&) = delete;

    DebugProxy(DebugProxy&& other) noexcept : handle_(other.handle_) { other.handle_ = nullptr; }
    DebugProxy& operator=(DebugProxy&& other) noexcept {
        if (this != &other) {
            reset();
            handle_       = other.handle_;
            other.handle_ = nullptr;
        }
        return *this;
    }

    ~DebugProxy() { reset(); }

    // Factory: connect over RSD (borrows adapter & handshake; does not consume
    // them)
    static Result<DebugProxy, FfiError>   connect_rsd(Adapter& adapter, RsdHandshake& rsd);

    // Factory: consume a ReadWrite stream (fat pointer)
    static Result<DebugProxy, FfiError>   from_readwrite_ptr(::ReadWriteOpaque* consumed);

    // Convenience: consume a C++ ReadWrite wrapper by releasing it into the ABI
    static Result<DebugProxy, FfiError>   from_readwrite(ReadWrite&& rw);

    // API
    Result<Option<std::string>, FfiError> send_command(const std::string&              name,
                                                       const std::vector<std::string>& argv);

    Result<Option<std::string>, FfiError> read_response();

    Result<void, FfiError>                send_raw(const std::vector<uint8_t>& data);

    // Reads up to `len` bytes; ABI returns a heap C string (we treat as bytes →
    // string)
    Result<Option<std::string>, FfiError> read(std::size_t len);

    // Sets argv, returns textual reply (OK/echo/etc)
    Result<Option<std::string>, FfiError> set_argv(const std::vector<std::string>& argv);

    Result<void, FfiError>                send_ack();
    Result<void, FfiError>                send_nack();

    // No error object in ABI; immediate effect
    void set_ack_mode(bool enabled) { ::debug_proxy_set_ack_mode(handle_, enabled ? 1 : 0); }

    ::DebugProxyHandle* raw() const { return handle_; }

  private:
    explicit DebugProxy(::DebugProxyHandle* h) : handle_(h) {}

    void reset() {
        if (handle_) {
            ::debug_proxy_free(handle_);
            handle_ = nullptr;
        }
    }

    ::DebugProxyHandle* handle_ = nullptr;
};

// Small helper that owns a DebugserverCommandHandle
class DebugCommand {
  public:
    DebugCommand()                               = default;
    DebugCommand(const DebugCommand&)            = delete;
    DebugCommand& operator=(const DebugCommand&) = delete;

    DebugCommand(DebugCommand&& other) noexcept : handle_(other.handle_) {
        other.handle_ = nullptr;
    }
    DebugCommand& operator=(DebugCommand&& other) noexcept {
        if (this != &other) {
            reset();
            handle_       = other.handle_;
            other.handle_ = nullptr;
        }
        return *this;
    }

    ~DebugCommand() { reset(); }

    static Option<DebugCommand> make(const std::string& name, const std::vector<std::string>& argv);

    ::DebugserverCommandHandle* raw() const { return handle_; }

  private:
    explicit DebugCommand(::DebugserverCommandHandle* h) : handle_(h) {}

    void reset() {
        if (handle_) {
            ::debugserver_command_free(handle_);
            handle_ = nullptr;
        }
    }

    ::DebugserverCommandHandle* handle_ = nullptr;
};

} // namespace IdeviceFFI
