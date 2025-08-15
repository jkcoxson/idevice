// Jackson Coxson

#include <idevice++/remote_server.hpp>

namespace IdeviceFFI {

std::optional<RemoteServer> RemoteServer::from_socket(ReadWrite&& rw, FfiError& err) {
    RemoteServerHandle* out = nullptr;

    // Rust consumes the stream regardless of result, release BEFORE the call
    ReadWriteOpaque*    raw = rw.release();

    if (IdeviceFfiError* e = ::remote_server_new(raw, &out)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return RemoteServer::adopt(out);
}

std::optional<RemoteServer>
RemoteServer::connect_rsd(Adapter& adapter, RsdHandshake& rsd, FfiError& err) {
    RemoteServerHandle* out = nullptr;
    if (IdeviceFfiError* e = ::remote_server_connect_rsd(adapter.raw(), rsd.raw(), &out)) {
        err = FfiError(e);
        return std::nullopt;
    }
    return RemoteServer::adopt(out);
}

} // namespace IdeviceFFI
