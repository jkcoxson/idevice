// Jackson Coxson

#pragma once

#include <idevice++/bindings.hpp>

namespace IdeviceFFI {

// A move-only holder for a fat-pointer stream. It does NOT free on destruction.
// Always pass ownership to an FFI that consumes it by calling release().
class ReadWrite {
  public:
    ReadWrite() noexcept : ptr_(nullptr) {}
    explicit ReadWrite(ReadWriteOpaque* p) noexcept : ptr_(p) {}

    ReadWrite(const ReadWrite&)            = delete;
    ReadWrite& operator=(const ReadWrite&) = delete;

    ReadWrite(ReadWrite&& other) noexcept : ptr_(other.ptr_) { other.ptr_ = nullptr; }
    ReadWrite& operator=(ReadWrite&& other) noexcept {
        if (this != &other) {
            ptr_       = other.ptr_;
            other.ptr_ = nullptr;
        }
        return *this;
    }

    ~ReadWrite() noexcept = default; // no dtor – Rust consumers own free/drop

    ReadWriteOpaque* raw() const noexcept { return ptr_; }
    ReadWriteOpaque* release() noexcept {
        auto* p = ptr_;
        ptr_    = nullptr;
        return p;
    }

    static ReadWrite adopt(ReadWriteOpaque* p) noexcept { return ReadWrite(p); }

    explicit         operator bool() const noexcept { return ptr_ != nullptr; }

  private:
    ReadWriteOpaque* ptr_;
};

} // namespace IdeviceFFI
