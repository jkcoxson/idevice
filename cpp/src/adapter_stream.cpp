// Jackson Coxson

#include <idevice++/adapter_stream.hpp>
#include <idevice++/option.hpp>

namespace IdeviceFFI {

Result<void, FfiError> AdapterStream::close() {
  if (!h_)
    return Ok();

  FfiError e(::adapter_close(h_));
  if (e) {
    return Err(e);
  }

  h_ = nullptr;
  return Ok();
}

Result<void, FfiError> AdapterStream::send(const uint8_t *data, size_t len) {
  if (!h_)
    return Err(FfiError::NotConnected());
  FfiError e(::adapter_send(h_, data, len));
  if (e) {
    return Err(e);
  }
  return Ok();
}

Result<std::vector<uint8_t>, FfiError> AdapterStream::recv(size_t max_hint) {
  if (!h_)
    return Err(FfiError::NotConnected());

  if (max_hint == 0)
    max_hint = 2048;

  std::vector<uint8_t> out(max_hint);
  size_t actual = 0;

  FfiError e(::adapter_recv(h_, out.data(), &actual, out.size()));
  if (e) {
    return Err(e);
  }

  out.resize(actual);
  return Ok(std::move(out));
}

} // namespace IdeviceFFI
