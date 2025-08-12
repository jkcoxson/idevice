// Jackson Coxson

#ifndef IDEVICE_USBMUXD_HPP
#define IDEVICE_USBMUXD_HPP

#include "ffi.hpp"
#include "idevice++.hpp"
#include "pairing_file.hpp"
#include <cstdint>
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

/// @brief A C++ wrapper for a UsbmuxdAddrHandle.
/// @details This class manages the memory of a UsbmuxdAddrHandle pointer.
/// It is non-copyable but is movable.
class UsbmuxdAddr {
  public:
    /// @brief Creates a new TCP usbmuxd address.
    /// @param addr The socket address to connect to.
    /// @param addr_len The length of the socket address.
    /// @param err An error that will be populated on failure.
    /// @return A UsbmuxdAddr on success, std::nullopt on failure.
    static std::optional<UsbmuxdAddr>
    tcp_new(const sockaddr* addr, socklen_t addr_len, FfiError& err) {
        UsbmuxdAddrHandle* handle = nullptr;
        IdeviceFfiError*   e      = idevice_usbmuxd_tcp_addr_new(addr, addr_len, &handle);
        if (e) {
            err = FfiError::from(e);
            return std::nullopt;
        }
        return UsbmuxdAddr(handle);
    }

#if defined(__unix__) || defined(__APPLE__)
    /// @brief Creates a new Unix socket usbmuxd address.
    /// @param path The path to the unix socket.
    /// @param err An error that will be populated on failure.
    /// @return A UsbmuxdAddr on success, std::nullopt on failure.
    static std::optional<UsbmuxdAddr> unix_new(const std::string& path, FfiError& err) {
        UsbmuxdAddrHandle* handle = nullptr;
        IdeviceFfiError*   e      = idevice_usbmuxd_unix_addr_new(path.c_str(), &handle);
        if (e) {
            err = FfiError::from(e);
            return std::nullopt;
        }
        return UsbmuxdAddr(handle);
    }
#endif

    ~UsbmuxdAddr() {
        if (handle_) {
            idevice_usbmuxd_addr_free(handle_);
        }
    }

    // Delete copy constructor and assignment operator
    UsbmuxdAddr(const UsbmuxdAddr&)            = delete;
    UsbmuxdAddr& operator=(const UsbmuxdAddr&) = delete;

    // Define move constructor and assignment operator
    UsbmuxdAddr(UsbmuxdAddr&& other) noexcept : handle_(other.handle_) { other.handle_ = nullptr; }
    UsbmuxdAddr& operator=(UsbmuxdAddr&& other) noexcept {
        if (this != &other) {
            if (handle_) {
                idevice_usbmuxd_addr_free(handle_);
            }
            handle_       = other.handle_;
            other.handle_ = nullptr;
        }
        return *this;
    }

    /// @brief Gets the raw handle.
    /// @return The raw UsbmuxdAddrHandle pointer.
    UsbmuxdAddrHandle* raw() const { return handle_; }

  private:
    explicit UsbmuxdAddr(UsbmuxdAddrHandle* handle) : handle_(handle) {}
    UsbmuxdAddrHandle* handle_;
};

class UsbmuxdConnectionType {
  public:
    enum Value : uint8_t { Usb = 1, Network = 2, Unknown = 3 };
    explicit UsbmuxdConnectionType(uint8_t v) : _value(static_cast<Value>(v)) {}

    std::string to_string() {
        switch (_value) {
        case UsbmuxdConnectionType::Usb:
            return "USB";
        case UsbmuxdConnectionType::Network:
            return "Network";
        case UsbmuxdConnectionType::Unknown:
            return "Unknown";
        default:
            return "UnknownEnumValue";
        }
    }

    Value value() const { return _value; }

    bool  operator==(Value other) const { return _value == other; }

  private:
    Value _value;
};

class UsbmuxdDevice {
  public:
    ~UsbmuxdDevice() {
        if (handle_) {
            idevice_usbmuxd_device_free(handle_);
        }
    }
    // Delete copy constructor and assignment operator
    UsbmuxdDevice(const UsbmuxdDevice&)            = delete;
    UsbmuxdDevice& operator=(const UsbmuxdDevice&) = delete;

    // Define move constructor and assignment operator
    UsbmuxdDevice(UsbmuxdDevice&& other) noexcept : handle_(other.handle_) {
        other.handle_ = nullptr;
    }
    UsbmuxdDevice& operator=(UsbmuxdDevice&& other) noexcept {
        if (this != &other) {
            if (handle_) {
                idevice_usbmuxd_device_free(handle_);
            }
            handle_       = other.handle_;
            other.handle_ = nullptr;
        }
        return *this;
    }

    /// @brief Gets the raw handle.
    /// @return The raw UsbmuxdConnectionHandle pointer.
    UsbmuxdDeviceHandle*       raw() const { return handle_; }

    std::optional<std::string> get_udid() {
        char* udid = idevice_usbmuxd_device_get_udid(handle_);
        if (udid) {
            std::string cppUdid(udid);
            idevice_string_free(udid);
            return cppUdid;
        } else {
            return std::nullopt;
        }
    }

    std::optional<uint32_t> get_id() {
        uint32_t id = idevice_usbmuxd_device_get_device_id(handle_);
        if (id == 0) {
            return std::nullopt;
        } else {
            return id;
        }
    }

    std::optional<UsbmuxdConnectionType> get_connection_type() {
        u_int8_t t = idevice_usbmuxd_device_get_connection_type(handle_);
        if (t == 0) {
            return std::nullopt;
        } else {
        }
        return static_cast<UsbmuxdConnectionType>(t);
    }
    explicit UsbmuxdDevice(UsbmuxdDeviceHandle* handle) : handle_(handle) {}

  private:
    UsbmuxdDeviceHandle* handle_;
};

/// @brief A C++ wrapper for a UsbmuxdConnectionHandle.
/// @details This class manages the memory of a UsbmuxdConnectionHandle pointer.
/// It is non-copyable but is movable.
class UsbmuxdConnection {
  public:
    /// @brief Creates a new TCP usbmuxd connection.
    /// @param addr The socket address to connect to.
    /// @param addr_len The length of the socket address.
    /// @param tag A tag that will be returned by usbmuxd responses.
    /// @param err An error that will be populated on failure.
    /// @return A UsbmuxdConnection on success, std::nullopt on failure.
    static std::optional<UsbmuxdConnection>
    tcp_new(const idevice_sockaddr* addr, idevice_socklen_t addr_len, uint32_t tag, FfiError& err) {
        UsbmuxdConnectionHandle* handle = nullptr;
        IdeviceFfiError* e = idevice_usbmuxd_new_tcp_connection(addr, addr_len, tag, &handle);
        if (e) {
            err = FfiError::from(e);
            return std::nullopt;
        }
        return UsbmuxdConnection(handle);
    }

#if defined(__unix__) || defined(__APPLE__)
    /// @brief Creates a new Unix socket usbmuxd connection.
    /// @param path The path to the unix socket.
    /// @param tag A tag that will be returned by usbmuxd responses.
    /// @param err An error that will be populated on failure.
    /// @return A UsbmuxdConnection on success, std::nullopt on failure.
    static std::optional<UsbmuxdConnection>
    unix_new(const std::string& path, uint32_t tag, FfiError& err) {
        UsbmuxdConnectionHandle* handle = nullptr;
        IdeviceFfiError* e = idevice_usbmuxd_new_unix_socket_connection(path.c_str(), tag, &handle);
        if (e) {
            err = FfiError::from(e);
            return std::nullopt;
        }
        return UsbmuxdConnection(handle);
    }
#endif

    /// @brief Creates a new usbmuxd connection using the default path.
    /// @param tag A tag that will be returned by usbmuxd responses.
    /// @param err An error that will be populated on failure.
    /// @return A UsbmuxdConnection on success, std::nullopt on failure.
    static std::optional<UsbmuxdConnection> default_new(uint32_t tag, FfiError& err) {
        UsbmuxdConnectionHandle* handle = nullptr;
        IdeviceFfiError*         e      = idevice_usbmuxd_new_default_connection(tag, &handle);
        if (e) {
            err = FfiError::from(e);
            return std::nullopt;
        }
        return UsbmuxdConnection(handle);
    }

    /// @brief Gets a list of all connected devices.
    /// @param err An error that will be populated on failure.
    /// @return A vector of UsbmuxdDevice objects on success, std::nullopt on failure.
    std::optional<std::vector<UsbmuxdDevice>> get_devices(FfiError& err) {
        UsbmuxdDeviceHandle** devices = nullptr;
        int                   count   = 0;
        IdeviceFfiError*      e       = idevice_usbmuxd_get_devices(handle_, &devices, &count);
        if (e) {
            err = FfiError::from(e);
            return std::nullopt;
        }

        std::vector<UsbmuxdDevice> result;
        result.reserve(count);
        for (int i = 0; i < count; ++i) {
            result.emplace_back(devices[i]);
        }

        return result;
    }

    /// @brief Gets the BUID from the daemon.
    /// @param err An error that will be populated on failure.
    /// @return The BUID string on success, std::nullopt on failure.
    std::optional<std::string> get_buid(FfiError& err) {
        char*            buid_c = nullptr;
        IdeviceFfiError* e      = idevice_usbmuxd_get_buid(handle_, &buid_c);
        if (e) {
            err = FfiError::from(e);
            return std::nullopt;
        }
        std::string buid(buid_c);
        idevice_string_free(buid_c);
        return buid;
    }

    /// @brief Gets the pairing record for a device.
    /// @param udid The UDID of the target device.
    /// @param err An error that will be populated on failure.
    /// @return A PairingFile object on success, std::nullopt on failure.
    std::optional<PairingFile> get_pair_record(const std::string& udid, FfiError& err) {
        IdevicePairingFile* pf_handle = nullptr;
        IdeviceFfiError*    e = idevice_usbmuxd_get_pair_record(handle_, udid.c_str(), &pf_handle);
        if (e) {
            err = FfiError::from(e);
            return std::nullopt;
        }
        return PairingFile(pf_handle);
    }

    /// @brief Connects to a port on a given device.
    /// @details This operation consumes the UsbmuxdConnection object. After a successful call,
    /// this object will be invalid and should not be used.
    /// @param device_id The ID of the target device.
    /// @param port The port number to connect to.
    /// @param err An error that will be populated on failure.
    /// @return An Idevice connection object on success, std::nullopt on failure.
    std::optional<Idevice>
    connect_to_device(uint32_t device_id, uint16_t port, const std::string& path, FfiError& err) {
        if (!handle_) {
            // Can't connect with an invalid handle
            return std::nullopt;
        }

        IdeviceHandle*   idevice_handle = nullptr;
        IdeviceFfiError* e              = idevice_usbmuxd_connect_to_device(
            handle_, device_id, port, path.c_str(), &idevice_handle);

        // The handle is always consumed by the FFI call, so we must invalidate it.
        handle_ = nullptr;

        if (e) {
            err = FfiError::from(e);
            return std::nullopt;
        }

        return Idevice(idevice_handle);
    }

    ~UsbmuxdConnection() {
        if (handle_) {
            idevice_usbmuxd_connection_free(handle_);
        }
    }

    // Delete copy constructor and assignment operator
    UsbmuxdConnection(const UsbmuxdConnection&)            = delete;
    UsbmuxdConnection& operator=(const UsbmuxdConnection&) = delete;

    // Define move constructor and assignment operator
    UsbmuxdConnection(UsbmuxdConnection&& other) noexcept : handle_(other.handle_) {
        other.handle_ = nullptr;
    }
    UsbmuxdConnection& operator=(UsbmuxdConnection&& other) noexcept {
        if (this != &other) {
            if (handle_) {
                idevice_usbmuxd_connection_free(handle_);
            }
            handle_       = other.handle_;
            other.handle_ = nullptr;
        }
        return *this;
    }

    /// @brief Gets the raw handle.
    /// @return The raw UsbmuxdConnectionHandle pointer.
    UsbmuxdConnectionHandle* raw() const { return handle_; }

  private:
    explicit UsbmuxdConnection(UsbmuxdConnectionHandle* handle) : handle_(handle) {}
    UsbmuxdConnectionHandle* handle_;
};

} // namespace IdeviceFFI

#endif
