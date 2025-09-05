// Jackson Coxson

#include <cstring>
#include <idevice++/debug_proxy.hpp>

namespace IdeviceFFI {

// ---- helpers ----
static Option<std::string> take_cstring(char *p) {
  if (!p)
    return None;
  std::string s(p);
  ::idevice_string_free(p);
  return Some(s);
}

// ---- DebugCommand ----
Option<DebugCommand> DebugCommand::make(const std::string &name,
                                        const std::vector<std::string> &argv) {
  std::vector<const char *> c_argv;
  c_argv.reserve(argv.size());
  for (auto &a : argv)
    c_argv.push_back(a.c_str());

  auto *h = ::debugserver_command_new(
      name.c_str(),
      c_argv.empty() ? nullptr : const_cast<const char *const *>(c_argv.data()),
      c_argv.size());
  if (!h)
    return None;
  return Some(DebugCommand(h));
}

// ---- DebugProxy factories ----
Result<DebugProxy, FfiError> DebugProxy::connect_rsd(Adapter &adapter,
                                                     RsdHandshake &rsd) {
  ::DebugProxyHandle *out = nullptr;
  FfiError e(::debug_proxy_connect_rsd(adapter.raw(), rsd.raw(), &out));
  if (e) {
    return Err(e);
  }
  return Ok(DebugProxy(out));
}

Result<DebugProxy, FfiError>
DebugProxy::from_readwrite_ptr(::ReadWriteOpaque *consumed) {
  ::DebugProxyHandle *out = nullptr;
  FfiError e(::debug_proxy_new(consumed, &out));
  if (e) {
    return Err(e);
  }
  return Ok(DebugProxy(out));
}

Result<DebugProxy, FfiError> DebugProxy::from_readwrite(ReadWrite &&rw) {
  // Rust consumes the pointer regardless of outcome; release before calling
  return from_readwrite_ptr(rw.release());
}

// ---- DebugProxy API ----
Result<Option<std::string>, FfiError>
DebugProxy::send_command(const std::string &name,
                         const std::vector<std::string> &argv) {
  auto cmdRes = DebugCommand::make(name, argv);
  if (cmdRes.is_none()) {
    // treat as invalid arg
    FfiError err;
    err.code = -1;
    err.message = "debugserver_command_new failed";
    return Err(err);
  }
  auto cmd = std::move(cmdRes).unwrap();

  char *resp_c = nullptr;
  FfiError e(::debug_proxy_send_command(handle_, cmd.raw(), &resp_c));
  if (e) {
    return Err(e);
  }

  return Ok(take_cstring(resp_c));
}

Result<Option<std::string>, FfiError> DebugProxy::read_response() {
  char *resp_c = nullptr;
  FfiError e(::debug_proxy_read_response(handle_, &resp_c));
  if (e) {
    return Err(e);
  }
  return Ok(take_cstring(resp_c));
}

Result<void, FfiError> DebugProxy::send_raw(const std::vector<uint8_t> &data) {
  FfiError e(::debug_proxy_send_raw(handle_, data.data(), data.size()));
  if (e) {
    return Err(e);
  }
  return Ok();
}

Result<Option<std::string>, FfiError> DebugProxy::read(std::size_t len) {
  char *resp_c = nullptr;
  FfiError e(::debug_proxy_read(handle_, len, &resp_c));
  if (e) {
    return Err(e);
  }
  return Ok(take_cstring(resp_c));
}

Result<Option<std::string>, FfiError>
DebugProxy::set_argv(const std::vector<std::string> &argv) {
  std::vector<const char *> c_argv;
  c_argv.reserve(argv.size());
  for (auto &a : argv)
    c_argv.push_back(a.c_str());

  char *resp_c = nullptr;
  FfiError e(::debug_proxy_set_argv(
      handle_,
      c_argv.empty() ? nullptr : const_cast<const char *const *>(c_argv.data()),
      c_argv.size(), &resp_c));
  if (e) {
    return Err(e);
  }
  return Ok(take_cstring(resp_c));
}

Result<void, FfiError> DebugProxy::send_ack() {
  FfiError e(::debug_proxy_send_ack(handle_));
  if (e) {
    return Err(e);
  }
  return Ok();
}

Result<void, FfiError> DebugProxy::send_nack() {
  FfiError e(::debug_proxy_send_nack(handle_));
  if (e) {
    return Err(e);
  }
  return Ok();
}

} // namespace IdeviceFFI
