// Jackson Coxson

#ifndef IDEVICE_USBMUXD_HPP
#define IDEVICE_USBMUXD_HPP

#include <cstdint>
#include <idevice++/idevice.hpp>
#include <idevice++/option.hpp>
#include <idevice++/pairing_file.hpp>
#include <string>
#include <vector>

#ifdef _WIN32
#include <winsock2.h>
#include <ws2tcpip.h>
#else
#include <sys/socket.h>
#endif

namespace IdeviceFFI {

using AddrPtr =
    std::unique_ptr<UsbmuxdAddrHandle, FnDeleter<UsbmuxdAddrHandle, idevice_usbmuxd_addr_free>>;
using DevicePtr = std::unique_ptr<UsbmuxdDeviceHandle,
                                  FnDeleter<UsbmuxdDeviceHandle, idevice_usbmuxd_device_free>>;
using ConnectionPtr =
    std::unique_ptr<UsbmuxdConnectionHandle,
                    FnDeleter<UsbmuxdConnectionHandle, idevice_usbmuxd_connection_free>>;

class UsbmuxdAddr {
  public:
    static Result<UsbmuxdAddr, FfiError> tcp_new(const sockaddr* addr, socklen_t addr_len);
#if defined(__unix__) || defined(__APPLE__)
    static Result<UsbmuxdAddr, FfiError> unix_new(const std::string& path);
#endif
    static UsbmuxdAddr default_new();

    ~UsbmuxdAddr() noexcept                          = default;
    UsbmuxdAddr(UsbmuxdAddr&&) noexcept              = default;
    UsbmuxdAddr& operator=(UsbmuxdAddr&&) noexcept   = default;
    UsbmuxdAddr(const UsbmuxdAddr&)                  = delete;
    UsbmuxdAddr&       operator=(const UsbmuxdAddr&) = delete;

    UsbmuxdAddrHandle* raw() const noexcept { return handle_.get(); }
    UsbmuxdAddrHandle* release() noexcept { return handle_.release(); }
    static UsbmuxdAddr adopt(UsbmuxdAddrHandle* h) noexcept { return UsbmuxdAddr(h); }

  private:
    explicit UsbmuxdAddr(UsbmuxdAddrHandle* h) noexcept : handle_(h) {}
    AddrPtr handle_{};
};

class UsbmuxdConnectionType {
  public:
    enum class Value : uint8_t { Usb = 1, Network = 2, Unknown = 3 };
    explicit UsbmuxdConnectionType(uint8_t v) : _value(static_cast<Value>(v)) {}

    std::string to_string() const; // body in .cpp
    Value       value() const noexcept { return _value; }
    bool        operator==(Value other) const noexcept { return _value == other; }

  private:
    Value _value{Value::Unknown};
};

class UsbmuxdDevice {
  public:
    ~UsbmuxdDevice() noexcept                            = default;
    UsbmuxdDevice(UsbmuxdDevice&&) noexcept              = default;
    UsbmuxdDevice& operator=(UsbmuxdDevice&&) noexcept   = default;
    UsbmuxdDevice(const UsbmuxdDevice&)                  = delete;
    UsbmuxdDevice&       operator=(const UsbmuxdDevice&) = delete;

    static UsbmuxdDevice adopt(UsbmuxdDeviceHandle* h) noexcept { return UsbmuxdDevice(h); }

    UsbmuxdDeviceHandle* raw() const noexcept { return handle_.get(); }

    Option<std::string>  get_udid() const;
    Option<uint32_t>     get_id() const;
    Option<UsbmuxdConnectionType> get_connection_type() const;

  private:
    explicit UsbmuxdDevice(UsbmuxdDeviceHandle* h) noexcept : handle_(h) {}
    DevicePtr handle_{};

    friend class UsbmuxdConnection;
};

class PairingFile;

class UsbmuxdConnection {
  public:
    static Result<UsbmuxdConnection, FfiError>
    tcp_new(const idevice_sockaddr* addr, idevice_socklen_t addr_len, uint32_t tag);
#if defined(__unix__) || defined(__APPLE__)
    static Result<UsbmuxdConnection, FfiError> unix_new(const std::string& path, uint32_t tag);
#endif
    static Result<UsbmuxdConnection, FfiError> default_new(uint32_t tag);

    ~UsbmuxdConnection() noexcept                                                    = default;
    UsbmuxdConnection(UsbmuxdConnection&&) noexcept                                  = default;
    UsbmuxdConnection& operator=(UsbmuxdConnection&&) noexcept                       = default;
    UsbmuxdConnection(const UsbmuxdConnection&)                                      = delete;
    UsbmuxdConnection&                           operator=(const UsbmuxdConnection&) = delete;

    Result<std::vector<UsbmuxdDevice>, FfiError> get_devices() const;
    Result<std::string, FfiError>                get_buid() const;
    Result<PairingFile, FfiError>                get_pair_record(const std::string& udid);

    Result<Idevice, FfiError>
    connect_to_device(uint32_t device_id, uint16_t port, const std::string& path) &&;
    Result<Idevice, FfiError> connect_to_device(uint32_t, uint16_t, const std::string&) & = delete;
    Result<Idevice, FfiError>
    connect_to_device(uint32_t, uint16_t, const std::string&) const& = delete;

    UsbmuxdConnectionHandle* raw() const noexcept { return handle_.get(); }

  private:
    explicit UsbmuxdConnection(UsbmuxdConnectionHandle* h) noexcept : handle_(h) {}
    ConnectionPtr handle_{};
};

} // namespace IdeviceFFI
#endif
