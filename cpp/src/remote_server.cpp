// Jackson Coxson

#include <idevice++/remote_server.hpp>

namespace IdeviceFFI {

Result<RemoteServer, FfiError> RemoteServer::from_socket(ReadWrite&& rw) {
    RemoteServerHandle* out = nullptr;

    // Rust consumes the stream regardless of result, release BEFORE the call
    ReadWriteOpaque*    raw = rw.release();

    FfiError            e(::remote_server_new(raw, &out));
    if (e) {
        return Err(e);
    }
    return Ok(RemoteServer::adopt(out));
}

Result<RemoteServer, FfiError> RemoteServer::connect_rsd(Adapter& adapter, RsdHandshake& rsd) {
    RemoteServerHandle* out = nullptr;
    FfiError            e(::remote_server_connect_rsd(adapter.raw(), rsd.raw(), &out));
    if (e) {
        return Err(e);
    }
    return Ok(RemoteServer::adopt(out));
}

} // namespace IdeviceFFI
