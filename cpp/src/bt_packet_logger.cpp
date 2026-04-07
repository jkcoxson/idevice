// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/bt_packet_logger.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/provider.hpp>

namespace IdeviceFFI {

Result<BtPacketLogger, FfiError> BtPacketLogger::connect(Provider& provider) {
    BtPacketLoggerClientHandle* out = nullptr;
    FfiError                    e(::bt_packet_logger_connect(provider.raw(), &out));
    if (e) {
        provider.release();
        return Err(e);
    }
    return Ok(BtPacketLogger::adopt(out));
}

Result<BtPacketLogger, FfiError> BtPacketLogger::connect_rsd(AdapterHandle* adapter, RsdHandshakeHandle* handshake) {
    BtPacketLoggerClientHandle* out = nullptr;
    FfiError                    e(::bt_packet_logger_connect_rsd(adapter, handshake, &out));
    if (e) {
        return Err(e);
    }
    return Ok(BtPacketLogger::adopt(out));
}

Result<BtPacketLogger, FfiError> BtPacketLogger::from_socket(Idevice&& socket) {
    BtPacketLoggerClientHandle* out = nullptr;
    FfiError                    e(::bt_packet_logger_new(socket.raw(), &out));
    if (e) {
        return Err(e);
    }
    socket.release();
    return Ok(BtPacketLogger::adopt(out));
}

Result<Option<BtPacket>, FfiError> BtPacketLogger::next_packet() {
    BtPacketHandle* packet = nullptr;
    FfiError        e(::bt_packet_logger_next_packet(handle_.get(), &packet));
    if (e) {
        return Err(e);
    }
    if (!packet) {
        return Ok(Option<BtPacket>{});
    }

    BtPacket result;
    result.length   = packet->length;
    result.ts_secs  = packet->ts_secs;
    result.ts_usecs = packet->ts_usecs;
    result.kind     = packet->kind;
    if (packet->h4_data && packet->h4_data_len > 0) {
        result.h4_data.assign(packet->h4_data, packet->h4_data + packet->h4_data_len);
    }
    ::bt_packet_free(packet);

    return Ok(Option<BtPacket>(std::move(result)));
}

} // namespace IdeviceFFI
