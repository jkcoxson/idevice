// Jackson Coxson

use std::{
    ffi::c_int,
    net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr},
    os::raw::c_void,
};

use idevice::IdeviceError;
use libc::{sockaddr_in, sockaddr_in6};
use plist::Value;

pub(crate) fn c_socket_to_rust(
    addr: *const libc::sockaddr,
    addr_len: libc::socklen_t,
) -> Result<SocketAddr, IdeviceError> {
    Ok(unsafe {
        match (*addr).sa_family as c_int {
            libc::AF_INET => {
                if (addr_len as usize) < std::mem::size_of::<sockaddr_in>() {
                    log::error!("Invalid sockaddr_in size");
                    return Err(IdeviceError::FfiInvalidArg);
                }
                let addr_in = *(addr as *const sockaddr_in);
                let ip = std::net::Ipv4Addr::from(u32::from_be(addr_in.sin_addr.s_addr));
                let port = u16::from_be(addr_in.sin_port);
                std::net::SocketAddr::V4(std::net::SocketAddrV4::new(ip, port))
            }
            libc::AF_INET6 => {
                if addr_len as usize >= std::mem::size_of::<sockaddr_in6>() {
                    let addr_in6 = *(addr as *const sockaddr_in6);
                    let ip = std::net::Ipv6Addr::from(addr_in6.sin6_addr.s6_addr);
                    let port = u16::from_be(addr_in6.sin6_port);
                    std::net::SocketAddr::V6(std::net::SocketAddrV6::new(
                        ip,
                        port,
                        addr_in6.sin6_flowinfo,
                        addr_in6.sin6_scope_id,
                    ))
                } else {
                    log::error!("Invalid sockaddr_in6 size");
                    return Err(IdeviceError::FfiInvalidArg);
                }
            }
            _ => {
                log::error!("Unsupported socket address family: {}", (*addr).sa_family);
                return Err(IdeviceError::FfiInvalidArg);
            }
        }
    })
}

pub(crate) fn c_addr_to_rust(addr: *const libc::sockaddr) -> Result<IpAddr, IdeviceError> {
    unsafe {
        // Check the address family
        match (*addr).sa_family as c_int {
            libc::AF_INET => {
                // Convert sockaddr_in (IPv4) to IpAddr
                let sockaddr_in = addr as *const sockaddr_in;
                let ip = (*sockaddr_in).sin_addr.s_addr;
                let octets = u32::from_be(ip).to_be_bytes();
                Ok(IpAddr::V4(Ipv4Addr::new(
                    octets[0], octets[1], octets[2], octets[3],
                )))
            }
            libc::AF_INET6 => {
                // Convert sockaddr_in6 (IPv6) to IpAddr
                let sockaddr_in6 = addr as *const sockaddr_in6;
                let ip = (*sockaddr_in6).sin6_addr.s6_addr;
                Ok(IpAddr::V6(Ipv6Addr::from(ip)))
            }
            _ => {
                log::error!("Unsupported socket address family: {}", (*addr).sa_family);
                Err(IdeviceError::FfiInvalidArg)
            }
        }
    }
}

pub(crate) fn plist_to_libplist(v: &Value) -> *mut libc::c_void {
    let buf = Vec::new();
    let mut writer = std::io::BufWriter::new(buf);
    plist::to_writer_xml(&mut writer, v).unwrap();
    let buf = String::from_utf8(writer.into_inner().unwrap()).unwrap();
    let p = plist_plus::Plist::from_xml(buf).unwrap();
    let ptr = p.get_pointer();
    p.false_drop();
    ptr
}

pub(crate) fn libplist_to_plist(v: *mut c_void) -> Value {
    let v: plist_plus::Plist = v.into();
    let v_string = v.to_string();
    v.false_drop();

    let reader = std::io::Cursor::new(v_string.as_bytes());
    plist::from_reader(reader).unwrap()
}
