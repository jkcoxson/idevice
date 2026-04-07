// Jackson Coxson

#include <idevice++/dvt/network_monitor.hpp>

namespace IdeviceFFI {

Result<NetworkMonitor, FfiError> NetworkMonitor::create(RemoteServer& server) {
    NetworkMonitorHandle* out = nullptr;
    FfiError              e(::network_monitor_new(server.raw(), &out));
    if (e) return Err(e);
    return Ok(NetworkMonitor::adopt(out));
}

Result<void, FfiError> NetworkMonitor::start() {
    FfiError e(::network_monitor_start(handle_.get()));
    if (e) return Err(e);
    return Ok();
}

Result<void, FfiError> NetworkMonitor::stop() {
    FfiError e(::network_monitor_stop(handle_.get()));
    if (e) return Err(e);
    return Ok();
}

static CppSocketAddress from_c_addr(const IdeviceSocketAddress& c) {
    return CppSocketAddress{
        c.family,
        c.port,
        c.addr ? std::string(c.addr) : std::string(),
    };
}

Result<NetworkEvent, FfiError> NetworkMonitor::next_event() {
    IdeviceNetworkEvent* raw = nullptr;
    FfiError             e(::network_monitor_next_event(handle_.get(), &raw));
    if (e) return Err(e);

    NetworkEvent ev;
    ev.event_type       = raw->event_type;
    ev.interface_index  = raw->interface_index;
    ev.interface_name   = raw->interface_name ? std::string(raw->interface_name) : std::string();
    ev.local_addr       = from_c_addr(raw->local_addr);
    ev.remote_addr      = from_c_addr(raw->remote_addr);
    ev.pid              = raw->pid;
    ev.recv_buffer_size = raw->recv_buffer_size;
    ev.recv_buffer_used = raw->recv_buffer_used;
    ev.serial_number    = raw->serial_number;
    ev.kind             = raw->kind;
    ev.rx_packets       = raw->rx_packets;
    ev.rx_bytes         = raw->rx_bytes;
    ev.tx_packets       = raw->tx_packets;
    ev.tx_bytes         = raw->tx_bytes;
    ev.rx_dups          = raw->rx_dups;
    ev.rx_ooo           = raw->rx_ooo;
    ev.tx_retx          = raw->tx_retx;
    ev.min_rtt          = raw->min_rtt;
    ev.avg_rtt          = raw->avg_rtt;
    ev.connection_serial = raw->connection_serial;
    ev.time             = raw->time;
    ev.unknown_type     = raw->unknown_type;

    ::network_monitor_event_free(raw);
    return Ok(std::move(ev));
}

} // namespace IdeviceFFI
