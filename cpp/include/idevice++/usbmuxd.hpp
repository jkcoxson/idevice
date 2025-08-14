// Jackson Coxson

#ifndef IDEVICE_USBMUXD_HPP
#define IDEVICE_USBMUXD_HPP

#include <cstdint>
#include <idevice++/idevice.hpp>
#include <idevice++/pairing_file.hpp>
#include <optional>
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
    static std::optional<UsbmuxdAddr>
    tcp_new(const sockaddr* addr, socklen_t addr_len, FfiError& err);
#if defined(__unix__) || defined(__APPLE__)
    static std::optional<UsbmuxdAddr> unix_new(const std::string& path, FfiError& err);
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
    ~UsbmuxdDevice() noexcept                                  = default;
    UsbmuxdDevice(UsbmuxdDevice&&) noexcept                    = default;
    UsbmuxdDevice& operator=(UsbmuxdDevice&&) noexcept         = default;
    UsbmuxdDevice(const UsbmuxdDevice&)                        = delete;
    UsbmuxdDevice&             operator=(const UsbmuxdDevice&) = delete;

    static UsbmuxdDevice       adopt(UsbmuxdDeviceHandle* h) noexcept { return UsbmuxdDevice(h); }

    UsbmuxdDeviceHandle*       raw() const noexcept { return handle_.get(); }

    std::optional<std::string> get_udid() const;
    std::optional<uint32_t>    get_id() const;
    std::optional<UsbmuxdConnectionType> get_connection_type() const;

  private:
    explicit UsbmuxdDevice(UsbmuxdDeviceHandle* h) noexcept : handle_(h) {}
    DevicePtr handle_{};

    friend class UsbmuxdConnection;
};

class PairingFile;

class UsbmuxdConnection {
  public:
    static std::optional<UsbmuxdConnection>
    tcp_new(const idevice_sockaddr* addr, idevice_socklen_t addr_len, uint32_t tag, FfiError& err);
#if defined(__unix__) || defined(__APPLE__)
    static std::optional<UsbmuxdConnection>
    unix_new(const std::string& path, uint32_t tag, FfiError& err);
#endif
    static std::optional<UsbmuxdConnection> default_new(uint32_t tag, FfiError& err);

    ~UsbmuxdConnection() noexcept                                                 = default;
    UsbmuxdConnection(UsbmuxdConnection&&) noexcept                               = default;
    UsbmuxdConnection& operator=(UsbmuxdConnection&&) noexcept                    = default;
    UsbmuxdConnection(const UsbmuxdConnection&)                                   = delete;
    UsbmuxdConnection&                        operator=(const UsbmuxdConnection&) = delete;

    std::optional<std::vector<UsbmuxdDevice>> get_devices(FfiError& err) const;
    std::optional<std::string>                get_buid(FfiError& err) const;
    std::optional<PairingFile> get_pair_record(const std::string& udid, FfiError& err);

    std::optional<Idevice>
    connect_to_device(uint32_t device_id, uint16_t port, const std::string& path, FfiError& err) &&;
    std::optional<Idevice>
    connect_to_device(uint32_t, uint16_t, const std::string&, FfiError&) & = delete;
    std::optional<Idevice>
    connect_to_device(uint32_t, uint16_t, const std::string&, FfiError&) const& = delete;

    UsbmuxdConnectionHandle* raw() const noexcept { return handle_.get(); }

  private:
    explicit UsbmuxdConnection(UsbmuxdConnectionHandle* h) noexcept : handle_(h) {}
    ConnectionPtr handle_{};
};

} // namespace IdeviceFFI
#endif
