// Jackson Coxson

#ifndef IDEVICE_CPP
#define IDEVICE_CPP

#include <idevice++/ffi.hpp>
#include <idevice++/pairing_file.hpp>
#include <idevice++/result.hpp>
#include <string>

#if defined(_WIN32) && !defined(__MINGW32__)
// MSVC doesn't have BSD u_int* types
using u_int8_t  = std::uint8_t;
using u_int16_t = std::uint16_t;
using u_int32_t = std::uint32_t;
using u_int64_t = std::uint64_t;
#endif

namespace IdeviceFFI {

// Generic “bind a free function” deleter
template <class T, void (*FreeFn)(T*)> struct FnDeleter {
    void operator()(T* p) const noexcept {
        if (p) {
            FreeFn(p);
        }
    }
};

using IdevicePtr = std::unique_ptr<IdeviceHandle, FnDeleter<IdeviceHandle, idevice_free>>;

class Idevice {
  public:
    static Result<Idevice, FfiError> create(IdeviceSocketHandle* socket, const std::string& label);
#if defined(__unix__) || defined(__APPLE__)
    static Result<Idevice, FfiError> from_fd(int fd, const std::string& label);
#endif

    static Result<Idevice, FfiError>
    create_tcp(const sockaddr* addr, socklen_t addr_len, const std::string& label);

    // Methods
    Result<std::string, FfiError> get_type() const;
    Result<void, FfiError>        rsd_checkin();
    Result<void, FfiError>        start_session(const PairingFile& pairing_file);

    // Ownership/RAII
    ~Idevice() noexcept                      = default;
    Idevice(Idevice&&) noexcept              = default;
    Idevice& operator=(Idevice&&) noexcept   = default;
    Idevice(const Idevice&)                  = delete;
    Idevice&       operator=(const Idevice&) = delete;

    static Idevice adopt(IdeviceHandle* h) noexcept { return Idevice(h); }

    // Accessor
    IdeviceHandle* raw() const noexcept { return handle_.get(); }
    IdeviceHandle* release() noexcept { return handle_.release(); }

  private:
    explicit Idevice(IdeviceHandle* h) noexcept : handle_(h) {}
    IdevicePtr handle_{};
};

} // namespace IdeviceFFI
#endif
