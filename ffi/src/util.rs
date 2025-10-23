// Jackson Coxson

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use idevice::IdeviceError;

// portable FFI-facing types (only used in signatures)
#[allow(non_camel_case_types)]
#[repr(C)]
pub struct idevice_sockaddr {
    _priv: [u8; 0], // opaque; acts as "struct sockaddr" placeholder
}
#[cfg(unix)]
#[allow(non_camel_case_types)]
pub type idevice_socklen_t = libc::socklen_t;
#[cfg(windows)]
#[allow(non_camel_case_types)]
pub type idevice_socklen_t = i32;

// platform sockaddr aliases for implementation
#[cfg(unix)]
pub(crate) type SockAddr = libc::sockaddr;
#[cfg(windows)]
use windows_sys::Win32::Networking::WinSock as winsock;
#[cfg(windows)]
pub(crate) type SockAddr = winsock::SOCKADDR;

#[cfg(unix)]
use libc::{self, sockaddr_in, sockaddr_in6};

#[cfg(windows)]
use windows_sys::Win32::Networking::WinSock::{
    AF_INET, AF_INET6, SOCKADDR_IN as sockaddr_in, SOCKADDR_IN6 as sockaddr_in6,
};

#[cfg(unix)]
type SockLen = libc::socklen_t;
#[cfg(windows)]
type SockLen = i32; // socklen_t is an int on Windows

#[inline]
fn invalid_arg<T>() -> Result<T, IdeviceError> {
    Err(IdeviceError::FfiInvalidArg)
}

pub(crate) fn c_socket_to_rust(
    addr: *const SockAddr,
    addr_len: SockLen,
) -> Result<SocketAddr, IdeviceError> {
    if addr.is_null() {
        tracing::error!("null sockaddr");
        return invalid_arg();
    }

    unsafe {
        let family = (*addr).sa_family;

        #[cfg(unix)]
        match family as i32 {
            libc::AF_INET => {
                if (addr_len as usize) < std::mem::size_of::<sockaddr_in>() {
                    tracing::error!("Invalid sockaddr_in size");
                    return invalid_arg();
                }
                let a = &*(addr as *const sockaddr_in);
                let ip = Ipv4Addr::from(u32::from_be(a.sin_addr.s_addr));
                let port = u16::from_be(a.sin_port);
                Ok(SocketAddr::V4(std::net::SocketAddrV4::new(ip, port)))
            }
            libc::AF_INET6 => {
                if (addr_len as usize) < std::mem::size_of::<sockaddr_in6>() {
                    tracing::error!("Invalid sockaddr_in6 size");
                    return invalid_arg();
                }
                let a = &*(addr as *const sockaddr_in6);
                let ip = Ipv6Addr::from(a.sin6_addr.s6_addr);
                let port = u16::from_be(a.sin6_port);
                Ok(SocketAddr::V6(std::net::SocketAddrV6::new(
                    ip,
                    port,
                    a.sin6_flowinfo,
                    a.sin6_scope_id,
                )))
            }
            _ => {
                tracing::error!(
                    "Unsupported socket address family: {}",
                    (*addr).sa_family as i32
                );
                invalid_arg()
            }
        }

        #[cfg(windows)]
        match family {
            AF_INET => {
                if (addr_len as usize) < std::mem::size_of::<sockaddr_in>() {
                    tracing::error!("Invalid SOCKADDR_IN size");
                    return invalid_arg();
                }
                let a = &*(addr as *const sockaddr_in);
                // IN_ADDR is a union; use S_un.S_addr (network byte order)
                let ip_be = a.sin_addr.S_un.S_addr;
                let ip = Ipv4Addr::from(u32::from_be(ip_be));
                let port = u16::from_be(a.sin_port);
                Ok(SocketAddr::V4(std::net::SocketAddrV4::new(ip, port)))
            }
            AF_INET6 => {
                if (addr_len as usize) < std::mem::size_of::<sockaddr_in6>() {
                    tracing::error!("Invalid SOCKADDR_IN6 size");
                    return invalid_arg();
                }
                let a = &*(addr as *const sockaddr_in6);
                // IN6_ADDR is a union; read the 16 Byte array
                let bytes: [u8; 16] = a.sin6_addr.u.Byte;
                let ip = Ipv6Addr::from(bytes);
                let port = u16::from_be(a.sin6_port);
                let scope_id = a.Anonymous.sin6_scope_id;
                Ok(SocketAddr::V6(std::net::SocketAddrV6::new(
                    ip,
                    port,
                    a.sin6_flowinfo,
                    scope_id,
                )))
            }
            _ => {
                tracing::error!("Unsupported socket address family: {}", (*addr).sa_family);
                invalid_arg()
            }
        }
    }
}

pub(crate) fn c_addr_to_rust(addr: *const SockAddr) -> Result<IpAddr, IdeviceError> {
    if addr.is_null() {
        tracing::error!("null sockaddr");
        return invalid_arg();
    }

    unsafe {
        #[cfg(unix)]
        let family = (*addr).sa_family as i32;
        #[cfg(windows)]
        let family = (*addr).sa_family;

        #[cfg(unix)]
        match family {
            libc::AF_INET => {
                let a = &*(addr as *const sockaddr_in);
                let octets = u32::from_be(a.sin_addr.s_addr).to_be_bytes();
                Ok(IpAddr::V4(Ipv4Addr::new(
                    octets[0], octets[1], octets[2], octets[3],
                )))
            }
            libc::AF_INET6 => {
                let a = &*(addr as *const sockaddr_in6);
                Ok(IpAddr::V6(Ipv6Addr::from(a.sin6_addr.s6_addr)))
            }
            _ => {
                tracing::error!(
                    "Unsupported socket address family: {}",
                    (*addr).sa_family as i32
                );
                invalid_arg()
            }
        }

        #[cfg(windows)]
        match family {
            AF_INET => {
                let a = &*(addr as *const sockaddr_in);
                let ip_be = a.sin_addr.S_un.S_addr;
                Ok(IpAddr::V4(Ipv4Addr::from(u32::from_be(ip_be))))
            }
            AF_INET6 => {
                let a = &*(addr as *const sockaddr_in6);
                let bytes: [u8; 16] = a.sin6_addr.u.Byte;
                Ok(IpAddr::V6(Ipv6Addr::from(bytes)))
            }
            _ => {
                tracing::error!("Unsupported socket address family: {}", (*addr).sa_family);
                invalid_arg()
            }
        }
    }
}
