// Jackson Coxson

#include <idevice++/bindings.hpp>
#include <idevice++/ffi.hpp>
#include <idevice++/pcapd.hpp>
#include <idevice++/provider.hpp>

namespace IdeviceFFI {

Result<Pcapd, FfiError> Pcapd::connect(Provider& provider) {
    PcapdClientHandle* out = nullptr;
    FfiError           e(::pcapd_connect(provider.raw(), &out));
    if (e) {
        provider.release();
        return Err(e);
    }
    return Ok(Pcapd::adopt(out));
}

Result<Pcapd, FfiError> Pcapd::connect_rsd(AdapterHandle* adapter, RsdHandshakeHandle* handshake) {
    PcapdClientHandle* out = nullptr;
    FfiError           e(::pcapd_connect_rsd(adapter, handshake, &out));
    if (e) {
        return Err(e);
    }
    return Ok(Pcapd::adopt(out));
}

Result<Pcapd, FfiError> Pcapd::from_socket(Idevice&& socket) {
    PcapdClientHandle* out = nullptr;
    FfiError           e(::pcapd_new(socket.raw(), &out));
    if (e) {
        return Err(e);
    }
    socket.release();
    return Ok(Pcapd::adopt(out));
}

Result<DevicePacket, FfiError> Pcapd::next_packet() {
    DevicePacketHandle* packet = nullptr;
    FfiError            e(::pcapd_next_packet(handle_.get(), &packet));
    if (e) {
        return Err(e);
    }

    DevicePacket result;
    result.header_length     = packet->header_length;
    result.header_version    = packet->header_version;
    result.packet_length     = packet->packet_length;
    result.interface_type    = packet->interface_type;
    result.unit              = packet->unit;
    result.io                = packet->io;
    result.protocol_family   = packet->protocol_family;
    result.frame_pre_length  = packet->frame_pre_length;
    result.frame_post_length = packet->frame_post_length;
    if (packet->interface_name) {
        result.interface_name = packet->interface_name;
    }
    result.pid = packet->pid;
    if (packet->comm) {
        result.comm = packet->comm;
    }
    result.svc  = packet->svc;
    result.epid = packet->epid;
    if (packet->ecomm) {
        result.ecomm = packet->ecomm;
    }
    result.seconds      = packet->seconds;
    result.microseconds = packet->microseconds;
    if (packet->data && packet->data_len > 0) {
        result.data.assign(packet->data, packet->data + packet->data_len);
    }
    ::pcapd_device_packet_free(packet);

    return Ok(std::move(result));
}

} // namespace IdeviceFFI
