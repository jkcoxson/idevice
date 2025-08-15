// Jackson Coxson

#ifndef IDEVICE_REMOTE_SERVER_H
#define IDEVICE_REMOTE_SERVER_H

#pragma once
#include <idevice++/core_device_proxy.hpp>
#include <idevice++/idevice.hpp>
#include <idevice++/readwrite.hpp>
#include <idevice++/rsd.hpp>
#include <memory>
#include <optional>

namespace IdeviceFFI {

using RemoteServerPtr =
    std::unique_ptr<RemoteServerHandle, FnDeleter<RemoteServerHandle, remote_server_free>>;

class RemoteServer {
  public:
    // Factory: consumes the ReadWrite stream regardless of result
    static std::optional<RemoteServer> from_socket(ReadWrite&& rw, FfiError& err);

    // Factory: borrows adapter + handshake (neither is consumed)
    static std::optional<RemoteServer>
    connect_rsd(Adapter& adapter, RsdHandshake& rsd, FfiError& err);

    // RAII / moves
    ~RemoteServer() noexcept                           = default;
    RemoteServer(RemoteServer&&) noexcept              = default;
    RemoteServer& operator=(RemoteServer&&) noexcept   = default;
    RemoteServer(const RemoteServer&)                  = delete;
    RemoteServer&       operator=(const RemoteServer&) = delete;

    RemoteServerHandle* raw() const noexcept { return handle_.get(); }
    static RemoteServer adopt(RemoteServerHandle* h) noexcept { return RemoteServer(h); }

  private:
    explicit RemoteServer(RemoteServerHandle* h) noexcept : handle_(h) {}
    RemoteServerPtr handle_{};
};

} // namespace IdeviceFFI
#endif
