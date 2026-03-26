// Jackson Coxson

#pragma once
#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>
#include <memory>

namespace IdeviceFFI {

using AmfiPtr = std::unique_ptr<AmfiClientHandle, FnDeleter<AmfiClientHandle, amfi_client_free>>;

class Amfi {
  public:
    // Factory: connect via Provider
    static Result<Amfi, FfiError> connect(Provider& provider);

    // Factory: wrap an existing Idevice socket (consumes it on success)
    static Result<Amfi, FfiError> from_socket(Idevice&& socket);

    // Ops
    Result<void, FfiError>        reveal_developer_mode_option_in_ui();
    Result<void, FfiError>        enable_developer_mode();
    Result<void, FfiError>        accept_developer_mode();

    // RAII / moves
    ~Amfi() noexcept                         = default;
    Amfi(Amfi&&) noexcept                    = default;
    Amfi& operator=(Amfi&&) noexcept         = default;
    Amfi(const Amfi&)                        = delete;
    Amfi&             operator=(const Amfi&) = delete;

    AmfiClientHandle* raw() const noexcept { return handle_.get(); }
    static Amfi       adopt(AmfiClientHandle* h) noexcept { return Amfi(h); }

  private:
    explicit Amfi(AmfiClientHandle* h) noexcept : handle_(h) {}
    AmfiPtr handle_{};
};

} // namespace IdeviceFFI
