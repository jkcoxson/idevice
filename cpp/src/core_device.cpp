// Jackson Coxson

#include <idevice++/core_device_proxy.hpp>

namespace IdeviceFFI {

// ---- Factories ----

Result<CoreDeviceProxy, FfiError> CoreDeviceProxy::connect(Provider &provider) {
  CoreDeviceProxyHandle *out = nullptr;
  FfiError e(::core_device_proxy_connect(provider.raw(), &out));
  if (e) {
    return Err(e);
  }
  return Ok(CoreDeviceProxy::adopt(out));
}

Result<CoreDeviceProxy, FfiError>
CoreDeviceProxy::from_socket(Idevice &&socket) {
  CoreDeviceProxyHandle *out = nullptr;

  // Rust consumes the socket regardless of result → release BEFORE call
  IdeviceHandle *raw = socket.release();

  FfiError e(::core_device_proxy_new(raw, &out));
  if (e) {
    return Err(e);
  }
  return Ok(CoreDeviceProxy::adopt(out));
}

// ---- IO ----

Result<void, FfiError> CoreDeviceProxy::send(const uint8_t *data, size_t len) {
  FfiError e(::core_device_proxy_send(handle_.get(), data, len));
  if (e) {
    return Err(e);
  }
  return Ok();
}

Result<void, FfiError> CoreDeviceProxy::recv(std::vector<uint8_t> &out) {
  if (out.empty())
    out.resize(4096); // a reasonable default; caller can pre-size
  size_t actual = 0;
  FfiError e(
      ::core_device_proxy_recv(handle_.get(), out.data(), &actual, out.size()));
  if (e) {
    return Err(e);
  }
  out.resize(actual);
  return Ok();
}

// ---- Handshake ----

Result<CoreClientParams, FfiError>
CoreDeviceProxy::get_client_parameters() const {
  uint16_t mtu = 0;
  char *addr_c = nullptr;
  char *mask_c = nullptr;

  FfiError e(::core_device_proxy_get_client_parameters(handle_.get(), &mtu,
                                                       &addr_c, &mask_c));
  if (e) {
    return Err(e);
  }

  CoreClientParams params;
  params.mtu = mtu;
  if (addr_c) {
    params.address = addr_c;
    ::idevice_string_free(addr_c);
  }
  if (mask_c) {
    params.netmask = mask_c;
    ::idevice_string_free(mask_c);
  }
  return Ok(std::move(params));
}

Result<std::string, FfiError> CoreDeviceProxy::get_server_address() const {
  char *addr_c = nullptr;
  FfiError e(::core_device_proxy_get_server_address(handle_.get(), &addr_c));
  if (e) {
    return Err(e);
  }
  std::string s;
  if (addr_c) {
    s = addr_c;
    ::idevice_string_free(addr_c);
  }
  return Ok(s);
}

Result<uint16_t, FfiError> CoreDeviceProxy::get_server_rsd_port() const {
  uint16_t port = 0;
  FfiError e(::core_device_proxy_get_server_rsd_port(handle_.get(), &port));
  if (e) {
    return Err(e);
  }
  return Ok(port);
}

// ---- Adapter creation (consumes *this) ----

Result<Adapter, FfiError> CoreDeviceProxy::create_tcp_adapter() && {
  AdapterHandle *out = nullptr;

  // Rust consumes the proxy regardless of result → release BEFORE call
  CoreDeviceProxyHandle *raw = this->release();

  FfiError e(::core_device_proxy_create_tcp_adapter(raw, &out));
  if (e) {
    return Err(e);
  }
  return Ok(Adapter::adopt(out));
}

} // namespace IdeviceFFI
